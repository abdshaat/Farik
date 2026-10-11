# Standalone: what moved, as cards, newest first

Status: ready
Branch: `feat/today-moved-cards`
Spec: `docs/SPEC.md` the end of 5.7's last paragraph (line 340, Today's lists; nothing in the spec names "What moved" or `moved.since` yet), section 8's web UI (Today); `docs/design/web-ui.md` (Today's row, line 121)
Depends on: none (`main` at `15d67ec`)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-11 (two rounds, against `docs/standards/workflow.md` stage 2)

## Goal

The founder, 2026-10-11: "Under what moved since yesterday in the main endpoint, show only the 3 most recent actions with a pagination on the rest. The also must be structured as a card with a picture of the the agent and a time stamp next to it. The current setup is a Queue that shows the most recent actions on the bottom which is not a good UX. The most recent actions must be on top." Then, on the first boards: "Instead of scrolling left and right make it a show more button that show more actions to the bottom. It should go from top to bottom." Approved design ("Approved.", same day; canvas https://claude.ai/artifact/RK9zWTa5mt3Gn2mrYAjFdq, kept as `docs/design/mockups/TodayMoved.dc.html`, committed with this plan): Today's "What moved since yesterday" lists its moves newest first, three at a time; each is a card with the picture of who made the move, their name, their role tag when an agent, the time beside them, and the move's sentence under it; "Show more (<n> more)" under the cards adds the next three older ones below; once all show, and there were more than three, "Show fewer" goes back to the newest three. Out of scope: the sentences' wording (the founder took the defaults: kept as the store words them, the name in them too), the 24-hour window, times staying UTC as the team's lines say them.

## Decisions

- Who made a move comes from the daemon, not from parsing the sentence: `Moved` gains `by: String`, and `moved.since`'s rows gain `by`, required: `"human"`, `"catervas"`, or any other actor id as the log holds it, which is an agent's id or, where a move carried no agent id, the actor's wire name (`product_manager`, `scrum_master`, `assignee`, `reviewer`; `transitions.rs:1005-1017`, `actor_wire` `:1078`), shown as an id the team does not hold (below). Rejected: guessing the name from the line's first word (a task's title or "The" starts some lines, and a name can be two words).
- `by` for each kind of move: a task's move, its `requested_by`; `task.integrated`, `"catervas"` (Catervas adds it to the project); `human.accepted`, `"human"`; `agent.slept`, the envelope's agent id; a ceremony's post, its `author`; a post's `Wrote`, its agent; `Allowed`, `Stopped`, `Declined`, `"human"`; `Sent`, `Failed`, `Missed`, `PlanEnded`, `"catervas"` (Catervas tells of Buffer and of its own hand-over); every ads line, `"catervas"`. `"governor"` becomes `"catervas"` (as `who`, `activity.rs:309-313`, names both "Catervas"), and so does an `agent.slept` with no agent id on its envelope; `who` itself is unchanged, so that line's sentence stays as it is today (" reached its usage limit…", wording being out of scope), only its `by` being new. A ceremony's `author` is always an agent's id (the channel's `catervas` author is on `System` messages alone, `channel.rs:286`, which `moved_since` skips). Rejected: an absent `by` for the moves nobody on the team made (one more case for the page, for lines Catervas is the one telling).
- Order: the daemon keeps answering oldest first (the order a log reads; nothing else changes), and the page reverses it. Rejected: reversing in `moved_since`, which would reorder the store's three tests for no reader's gain.
- The card: `<li>` with a picture, then a head line (name in `strong`, `RoleTag` for an agent, `time`), then the sentence. An agent on the team with an `avatar`: `Avatar` at 48 (the component's sizes are 32, 48 and 64; the board's 40 is not one) with `name=""`, so its `alt` is empty (`Avatar.tsx` renders `alt={name}`; no package change), then its display name; an agent on the team with no `avatar` (`Agent.avatar` is optional, `TeamSetup.tsx:21`): no picture, name and role tag only, as `AgentEntry` (`Today.tsx:387`). `"human"`: a `<span className={styles.you} aria-hidden="true">` holding an inline SVG person, `width="24" height="24" viewBox="0 0 22 22" fill="none" stroke="currentColor" stroke-width="1.75"`, a `<circle cx="11" cy="8" r="3.5"/>` and a `<path d="M4 19c1-3.6 3.8-5.5 7-5.5s6 1.9 7 5.5"/>`; `.you` is 48 by 48, `flex-shrink: 0`, a centring inline-flex, `background: var(--catervas-color-ink)`, `color: var(--catervas-color-page)`, `border-radius: var(--catervas-radius-control)`; then "You" (`movedYou`). `"catervas"`: `logo-mark-1254.png` at 48, `alt=""`, named by `t("brand")`, as `AdsRow.tsx:125` draws Catervas's own row. An agent id the team does not hold (retired and removed): its id as the name, no picture, no role tag. The picture's `alt` is empty where the name follows it, so a screen reader reads the name once.
- Time: today's `HH:MM`; an earlier date, "Yesterday HH:MM" (`movedYesterday`, "Yesterday {time}"), the list holding a day; both UTC from `at`, `time`'s `dateTime` the row's `at`. `yesterdayShort` ("Yday") and `yesterday` are deleted, Today being their only reader.
- Show more: `useState(3)`; the list shows the first `shown` of the reversed rows. One `Button` (`kind` default), rendered while there are more than three rows, inside `<div className={styles.movedMore}>` (`.movedMore > button { width: 100%; }`, `Button` taking no `className`): while rows are hidden its words are "Show more ({n} more)" (`movedMore`) and it adds three; once all show its words are "Show fewer" (`movedFewer`) and it sets three again. Being one element whose words switch, it keeps focus when pressed (the new cards land above it). Rejected: pages with Newer and Older (the founder's first boards, replaced), a "load more" from the daemon (a day's moves are already in hand).
- Where the board and this plan differ, the plan wins: pictures at 48 (not 40), the card's radius `--catervas-radius-raised`, and the button's words plain, "Show more ({n} more)", with no chevron and no muted part.
- Style: `.moved` stays the bulletless grid; `.moved li` becomes the card (surface, `rule` border, `--catervas-radius-raised`, `space-3` `space-4` padding, flex with `space-3` gap); the head line wraps; `time` keeps the code face and `ink-muted`. The `Button` at the list's end takes the row's width.
- Spec revision 0.87 (2026-10-11), a header sentence at `docs/SPEC.md:3`, and one sentence at the end of 5.7's last paragraph (line 340): Today's "What moved since yesterday" shows the moves newest first as cards with who made each, three at first, "Show more" adding three; `moved.since` gives `by`. `docs/design/web-ui.md`'s Today row (line 121) says "What moved" is cards, newest first, with Show more.

## File map

```
crates/store/src/activity.rs                    modifies, tests  (Task 1)  Moved.by, each move's by
crates/runtime/src/daemon/gates.rs              modifies         (Task 1)  moved.since gives by
docs/schemas/rpc.schema.json                    modifies         (Task 1)  movedSinceResult's by, required
apps/web/src/pages/Today.tsx, Today.module.css  modifies         (Task 2)  the cards and Show more
apps/web/src/pages/Today.test.tsx               tests            (Task 2)
apps/web/src/strings/en.ts                      modifies         (Task 2)
docs/design/mockups/TodayMoved.dc.html          creates          (with this plan)  the approved board
docs/SPEC.md, docs/design/web-ui.md             modifies         (Task 3)
```

## Interfaces

Consumes: `moved_since`, `post_moves`, `ad_moves`, `name_of` (store); `Avatar`, `RoleTag`, `Button` (`@catervas/ui`); `Agent` (`TeamSetup.tsx`); `today`, `answerQuery`, `expectNoAxeViolations` (tests).

Produces:

```rust
pub struct Moved { pub at: DateTime<Utc>, pub by: String, pub line: String }
```
```ts
type Moved = { at: string; by: string; line: string }; // Today.tsx:96; toCamel maps keys only, and `by` is one word
```
```json
"moved.since" → { "moved": [ { "at": "<date-time>", "by": "human|catervas|<another actor id>", "line": "<sentence>" } ] }
```

## Tasks

### Task 1: each move says who made it

Files: `crates/store/src/activity.rs`, `crates/runtime/src/daemon/gates.rs`, `docs/schemas/rpc.schema.json`

Tests:

- `says_what_moved_since` asserts `(at, by, line)`: `linus` for Linus's move, `human` for the human's move and acceptances, `catervas` for both integrations (`integrated_by: governor`), `grace` for the nap, `mira` for the standup; and two new events at `at(10, 8)`: recorded first, `board.moved(at(10, 8), "CTV-1", ("rejected", "in_progress"), "governor", (Some("linus"), Some("grace")))`, `(at(10, 8), "catervas", "Catervas moved “Add a login form” to In progress")`, then, recorded after it (`moved_since` orders by `seq`), an `agent.slept` with no agent id until 15:00, `(at(10, 8), "catervas", " reached its usage limit and will pick up again at 15:00")`.
- `moved_tells_of_the_posts` asserts `by` per line: `kai` for each "Kai wrote", `human` for allowed, stopped and not allowed, `catervas` for handed, did not take, did not go out, and stopped for the plan's end.
- `says_what_moved_with_the_ads` asserts `by` is `catervas` on every line.
- The daemon's shape test (`gates.rs:4069-4076`, the `movedSinceResult` query) holds the answer to the schema, which now requires `by` (`movedSinceResult` has `additionalProperties: false`, so the daemon's `by` fails it until the schema gains it, and the schema's `required` fails the daemon until it sends it).

- [x] `feat(store): say who made each move`

### Task 2: the cards, newest first, three at a time

Files: `Today.tsx`, `Today.module.css`, `Today.test.tsx`, `en.ts` (`movedYou`, `movedYesterday`, `movedMore`, `movedFewer` added; `yesterday`, `yesterdayShort` deleted)
Consumes: Task 1's `by`

Tests (`Today.test.tsx`; `today()`'s fixture team holds the agents named):

- `says_what_moved` (rewritten) — rows oldest first from the daemon, one by an agent today, one by `human`, one by `catervas` yesterday: the list's first card is the newest; an agent's card has its avatar image named by nothing extra (`alt=""`), its name, its role tag and `HH:MM` in a `time` whose `dateTime` is the row's `at`; the human's card reads "You"; Catervas's reads "Catervas" with the mark; yesterday's time reads "Yesterday HH:MM"; `since` is 24 hours back; axe passes.
- `shows_three_moves_then_more` — seven rows: three cards, newest first, and "Show more (4 more)"; pressed, six cards in order, the first three unchanged, "Show more (1 more)"; pressed, seven and "Show fewer", no Show more; pressed, three again; after each press `document.activeElement` is the button; axe passes at each.
- `shows_no_button_for_three_or_fewer` — three rows: three cards, no Show more, no Show fewer.
- `names_an_agent_the_team_does_not_hold` — `by` an id not on the team (`assignee`): the card reads the id, with no image and no role tag.
- `draws_the_person_and_catervas` — the `human` card holds the `aria-hidden` tile and no `img`; the `catervas` card's `img` is the mark with `alt=""`; an agent on the team without `avatar` has no `img` and has its role tag.
- The empty case (`movedNone`) is unchanged.

- [ ] `feat(web): show what moved as cards, newest first, three at a time`

### Task 3: the spec and the design say so

Files: `docs/SPEC.md` (0.87 header sentence; the sentence at the end of 5.7's last paragraph, line 340), `docs/design/web-ui.md` (Today's row)

Tests: none (docs).

- [ ] `docs(spec): say what moved shows as cards, newest first`

## Verification

```
cargo xtask check
# expected: xtask check: ok (this session's container has no Docker, so not --integration;
# nothing here touches the sandbox)
pnpm check
# expected: exit 0, every package's tests passed
pnpm --filter @catervas/web e2e
# expected: every journey passed
```

Then the founder opens Today with more than three moves since yesterday, sees the newest on top, and presses Show more.
