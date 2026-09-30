# Phase 6, step 10: Channel

Status: ready
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` section 5.9, F7
Depends on: steps 01 to 09 of this phase
Readiness confirmed by: fresh-session reviewer, 2026-09-29, round one: not ready on one planner decision (reading the newest page), settled below; round two found it settled (ready with findings, folded in).

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The user reads the team channel in the browser, live:
- agents' reactions and replies;
- Farik's system lines;
- the user's own posts;
- the ceremonies, grouped into collapsible threads titled in plain words;
- task ids that link to their task.

The user posts to the team and mentions an agent with `@`, and that agent's reply appears by itself. Today gains the "In the channel" preview that step 08 left out, and the sprint page's meeting links open their thread.

Out of scope: one-on-ones ("Talk to one person" is phase 8), notifications (phase 8), and replying in a thread, since the human's `message_post` has only `text` (spec 5.9).

## Decisions

- **The query.** `channel.messages { before_seq?, limit }` has a limit of 1 to 200, default 100. It answers the newest page `[{ seq, at, author, kind, text, mentions, thread, in_reply_to, task_id }]`, oldest first within the page. `EventQuery` gains `before_seq: Option<u64>` and `newest_first: bool`: with `newest_first`, `limit` takes the highest sequence numbers below `before_seq`. The runtime reads `{ kinds: [message.posted], before_seq, newest_first: true, limit }` and reverses the page. The existing `ponytail:` note on `farik channel` reading every message stays, and this query reads one page.
- **Live.** New `message.posted` events come from step 04's `useEvents` subscription, and are appended without another query. "Show earlier messages" pages back with `before_seq`.
- **Authors.**
  - An agent's post shows its avatar, name and role tag.
  - Farik's system lines show in small muted text, with no avatar.
  - The user's own posts are labelled "You".
  - `@human` in a text is shown as "@you". Mentions of agents are shown as "@<Name>" and highlighted.
- **Threads.** Ceremony messages (`kind: ceremony`) that share a `thread` and a UTC day form one collapsible block, titled "<Planning | Standup | Review | Looking back>, <weekday>: N posted". Blocks start collapsed, except today's standup. Each block has an anchor `#thread-<thread>-<date>` for the sprint page's "Read it" links, which step 09 left out and this step adds.
- **Links.** A message's `task_id`, and any `FRK-n` in its text, link to `/tasks/:id`. A task waiting on the user shows "waiting on you" after its link, from `waiting.list`.
- **Copy** is the mockups' word for word: the thread title "<Thread>, <weekday>: N people posted" ("1 person posted" for one); the composer hint "A mentioned agent answers here. To have work done, send a request instead."; the intro as the Channel mockup writes it; PhoneChannel's heading "Message the team". "Talk to one person" is left out (phase 8).
- **A reply** shows "Replying to <author of in_reply_to>" above its text.
- **Posting.** The composer is "Post to the team", with the mockup's hint, and "Post" sends `message_post { text }`. Typing `@` opens a listbox of the team's agents (arrow keys and Enter, `role="listbox"`), which inserts `@<id>`. Text over 2000 characters, counted in code points (`[...text].length`, as the runtime's `chars().count()`), is refused before sending, with the runtime's limit sentence. A refusal from the daemon shows under the box.
- **Intro.** The Channel mockup's intro, word for word, at the top (spec 5.9: only a filed request starts work).
- **Side panel.** "Meetings in this sprint" links to the open sprint's thread blocks, from `sprint.get`'s meetings. On a phone the side panel moves below the messages.
- **Today's preview.** Today shows the last two messages whose kind is not `system`, then "Open the channel".
- **The rail** gains Channel between Board and Team: six places on desktop, five on the phone bar.
- **Tests.** Vitest and axe on the page. Playwright `channel.spec.ts`, with the step 08 team and transcript `reply_to_a_mention`:
  1. post "@theo can you look at the menu page?";
  2. see the post labelled You;
  3. see Theo's reply appear by itself, with "Replying to You" (its FRK-1 link leads to "No page here", since the journey files no task; that is accepted);
  4. see the Today preview show it;
  5. take screenshots at 360 and 1280 px.

## File map

```
crates/store/src/event_log.rs (+ tests): EventQuery before_seq and newest_first (T1)
apps/web/src/app/App.tsx, shell/Shell.test.tsx: the /channel route, six places (T2)
crates/runtime/src/daemon/web.rs, docs/schemas/rpc.schema.json, crates/protocol/src/rpc.rs, packages/protocol-client/src/client.ts   modifies: channel.messages (T1)
apps/web/src/pages/{Channel,Today,SprintPage}.tsx, components/{MessageText,MentionBox}.tsx (+ css, tests), shell/Shell.tsx, strings/en.ts   creates / modifies (T2)
crates/cli/src/bin/farik-e2e-serve.rs, apps/web/e2e/channel.spec.ts                                   modifies / creates (T3)
docs/SPEC.md (F7), docs/plans/project-plan.md                                                          modifies (T3)
```

## Interfaces

Consumes: `useEvents` (step 04); `waiting.list` and `team.get`; `sprint.get` (step 09); command `message_post`.

Produces: the RPC query `channel.messages`; `renderMessageText(text: string, team: Agent[], waiting: WaitingRow[]): ReactNode` in `components/MessageText.tsx`.

## Tasks

### Task 1: The query

- `reads_the_newest_page_before_a_seq` (store): `newest_first` with `before_seq` and `limit`.
- `pages_the_channel`: the newest page first, oldest first within it; `before_seq` gives the page before; limit bounds.
- `carries_each_messages_links`: task_id, thread, in_reply_to, and mentions.

- [x] `feat(runtime): answer the channel a page at a time`

### Task 2: The channel page, the preview, and the meeting links

- `shows_each_kind_of_message`: an agent's post, a system line, and your post, with their labels.
- `groups_ceremonies_into_threads`: one block per thread and day, titled "<Thread>, <weekday>: N people posted" ("1 person posted" for one); only today's standup starts open.
- `links_tasks_and_mentions`: `FRK-2` in text links, and `@human` shows as "@you".
- `posts_and_mentions`: `@` opens the listbox; Enter inserts `@theo`; Post sends `message_post`; too long a text is refused before sending.
- `appends_new_messages_live`: a `message.posted` event adds a row without a query.
- `previews_the_channel_on_today`: the last two non-system messages.

- [x] `feat(web): add the channel, with threads, mentions and live posts`

### Task 3: The journey, the spec and the plan

- `channel.spec.ts`, as in the Tests decision.
- Spec F7: the channel in the web app.
- The project plan's step 10 line.

- [ ] `test(web): talk to the team through the real server and browser`

## Verification

```
cargo xtask check --integration
# expected: cargo 0 failed (T1 3 new); @farik/web: step 09's landed count plus 6; playwright: step 09's 8 plus 1 = 9 passed;
#   last line: xtask check: ok
```
