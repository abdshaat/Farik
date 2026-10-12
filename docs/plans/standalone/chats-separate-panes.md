# Standalone: the chat list and the chat scroll apart, newest first

Status: ready
Branch: `fix/chats-separate-panes`
Spec: `docs/SPEC.md` F8 (line 890, the Chats page; F7's channel is shown in it), section 8's web UI; `docs/design/web-ui.md` (the Channel row, line 127)
Depends on: none (`main` at `6e976fc`)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-12 (one round, against `docs/standards/workflow.md` stage 2; ready with findings 1 to 10, carried into this text)

## Goal

The founder, 2026-10-12: "The chat structure is not UX friendly. When I enter the channel endpoint both Chats and the person I'm chatting with are connected, and the user sees the oldest text rather than the newest. Separate both Chat and make sure the user start by seeing the most recent chat first, if the user wants to see old chats then he must scroll up." Asked, the founder chose (same day): two panes that scroll on their own on a wide screen; on a phone the row of faces stays above the chat, which scrolls on its own; older messages load by themselves on reaching the top; a new message while the user reads older ones leaves the view where it is and offers "New messages". Approved board ("Approved.", same day; canvas https://claude.ai/artifact/5JiH38npedhs6ozqNARsCY, kept as `docs/design/mockups/ChatsPanes.dc.html` and `PhoneChatsPanes.dc.html`, committed with this plan). When done: `/channel` and `/channel/<agent>` fill the window and the window itself does not scroll; the chat list scrolls inside itself; the open chat keeps its title at its top and its box at its bottom, its messages scrolling between them; a chat opens at its newest message; scrolling up to the top reads the page before, keeping the place; a new message is followed while the user is at the bottom, and otherwise "New messages" appears and goes to the newest. Out of scope: the messages' look, the chat list's rows, the Today preview, the queries (both already take `before_seq`).

## Decisions

- One component, `Feed` (`apps/web/src/components/Feed.tsx`), holds the scrolling for both chats; `Channel` and `OneToOne` keep their own lists, rows and paging state and pass them in. Rejected: the logic twice, once per page.
- The scroll box is `<section className={styles.scroller} aria-labelledby={labelledBy} tabIndex={0} onScroll>` with `// biome-ignore lint/a11y/noNoninteractiveTabindex: a scroll box must take focus to scroll by keyboard`, as `MarketingPlan.tsx:318-321` does; it holds, in order, the "Show earlier messages" button (while `more`), the failed alert (while `paging === "failed"`), `notes`, then `children` (the page's `<ol aria-label="Messages">`, unchanged). `labelledBy` is the id of the chat's `h1`: `channel-title` (Channel's, added) and `chat-title` (OneToOne's, added), so the box is the region "Team" or "Talking with <Name>". Channel passes its `gone` status ("channelMeetingGone") as `notes`; OneToOne passes none. The box shows focus: `.scroller:focus-visible { outline: 2px solid var(--catervas-color-focus); outline-offset: -2px }`. The first child of the scroller carries `margin-top: auto` in a flex column, so a short chat sits right above the box (rejected: `justify-content: flex-end`, which makes the top of an overflowing column unreachable).
- Where the view goes, decided in one `useLayoutEffect` that runs after every render (no dependency list), comparing `newest?.seq` and `oldest` with refs from the previous run, in this order: (1) the first render that has a `newest`: `scrollTop = scrollHeight`; (2) `oldest` changed (an earlier page arrived): `scrollTop += scrollHeight − the scrollHeight of the previous run`, so what the user saw stays where it was; (3) `newest.seq` changed: if `newest.mine` or the view was at the bottom, `scrollTop = scrollHeight`; else `behind` becomes true; (4) neither changed and the view was at the bottom: `scrollTop = scrollHeight` (the box or a row changed size: the faces row arriving after the first page, the waiting line, "Draft the product plan", a thread opened). The previous-scrollHeight ref is set at the end of every run. "At the bottom" is `scrollHeight − scrollTop − clientHeight <= 48`, kept in a ref, measured on every scroll event and after each move the effect makes (initially true). The scroller has `overflow-anchor: none`, so the browser's own anchoring does not move it twice. Rejected: browser scroll anchoring alone (not in every browser), `IntersectionObserver` and `ResizeObserver` (jsdom has neither; every size change here comes with a render of the page, which the effect follows).
- At the top: on a scroll event with `scrollTop <= 48`, `more`, and `paging === undefined`, `onEarlier()` is called. A failed read stops it until "Show earlier messages" is pressed (so a failing page is not asked in a loop); the button stays, for the keyboard and screen readers.
- "New messages": while `behind`, `<div className={styles.jump}>` (absolutely placed at the box's bottom centre, over the scroller, outside it) holds `<Button kind="primary">` with `t("chatsNewMessages")`, "New messages", and an `aria-hidden` down arrow SVG (`viewBox="0 0 24 24"`, `<path d="M12 5v14M5 12l7 7 7-7"/>`, stroke `currentColor`, width 2, 16 by 16). Pressing it sets `scrollTop = scrollHeight`, clears `behind` and moves focus to the scroller (the button then leaves). A scroll event that finds the view at the bottom clears `behind` too.
- The shell gives these pages the window: `Shell` adds `styles.fill` to its root while the path is `/channel` or starts with `/channel/` (as it reads `path` for `/team`, `Shell.tsx:52`); `.fill` is `height: 100dvh`, and `.fill .content` a flex column with `min-height: 0`, so the paused banner and the keys notice take their room and the page takes the rest. `Chats`' root is then `flex: 1 1 0; min-height: 0`: wide, a grid `300px minmax(0, 1fr)` with one row `minmax(0, 1fr)`, the list `overflow-y: auto`; narrow, rows `auto minmax(0, 1fr)`, the faces row then the chat. Rejected: `position: sticky` with a computed height (the phone's top bar, which wraps, would push the box under the bottom bar).
- The height passes down, rule by rule: Shell `.fill { height: 100dvh; min-height: 0; }` (else the root's `min-height: 100vh` wins on a phone with moving toolbars); `.fill .content { display: flex; flex-direction: column; min-height: 0; }`. `Chats.module.css`: `.wide` loses `align-items: start`; `.list { min-height: 0; overflow-y: auto; }`; `.open { display: flex; flex-direction: column; min-height: 0; }`. `Channel.module.css`: `.page { flex: 1 1 0; min-height: 0; grid-template-rows: minmax(0, 1fr); }`; `.main { display: flex; flex-direction: column; min-height: 0; }` (in place of its grid); `.side { overflow-y: auto; }` at 1440 px and wider; the folded aside in `.head` has `flex-basis: 100%`. `Chats.module.css` `.chat { flex: 1 1 0; min-height: 0; display: flex; flex-direction: column; }`. `Feed.module.css`: the root `position: relative; display: flex; flex-direction: column; flex: 1 1 0; min-height: 0;`; `.scroller { flex: 1 1 0; overflow-y: auto; overflow-anchor: none; display: flex; flex-direction: column; }`; `.scroller > :first-child { margin-top: auto; }`.
- Each chat is a flex column of its head, the `Feed` (`flex: 1 1 0; min-height: 0`), then what follows: Channel's composer; OneToOne's waiting line and composer. Channel's `.page` at 1440 px and wider keeps the meetings column beside, scrolling inside itself.
- Below 1440 px the meetings would have no room under the box: there the side panel is `<aside aria-labelledby="channel-meetings">` holding `<details>` (closed) with `<summary id="channel-meetings">` "Meetings in this sprint" and the same links, placed in the head under the intro; from 1440 px it is today's `<aside>` with its `h2 id="channel-meetings"`. Which one shows is read with `useMedia("(min-width: 1440px)")`. `Shell.tsx` gains `export function useMedia(query: string): boolean` (`useSyncExternalStore` on `matchMedia(query)`), and `useWide()` becomes `useMedia(WIDE)`. A meeting link opens its thread as today; opening it closes nothing.
- On a phone (`!useWide()`) a box keeps one row: Channel's "Enter sends" line and "To ask for work…" hint, and OneToOne's label and "Enter sends" line, take `channel.hidden` (visually hidden, still each box's description or label; Channel's label already does). Wide, nothing changes.
- A one-to-one pages back as the channel does: `more` is set from the first answer only (`messages.length === PAGE`), not from each later re-ask of the newest page (`round`); "Show earlier messages" and the top ask `chat.messages { agentId, beforeSeq: <oldest seq>, limit: 100 }`, merged by `bySeq`, `more` from its length; a failure shows `channelEarlierFailed`. Rejected: no paging (a chat past 100 messages would hide its start for good).
- Thresholds: 48 px for both the top and the bottom (one row's room; a hand that stops short still counts).
- Spec revision 0.103 (2026-10-12; the next free number when `main` has moved at merge time), a header sentence at `docs/SPEC.md:3` and F8's sentence "on a phone the list is one row of faces above the open chat" extended with what Goal says, its words final in Task 5. `docs/design/web-ui.md`'s Channel row gains one sentence.

## File map

```
apps/web/src/components/Feed.tsx, Feed.module.css   creates         (Task 1)  the scroll box, its moves, "New messages"
apps/web/src/components/Feed.test.tsx               tests           (Task 1)
apps/web/src/strings/en.ts                          modifies        (Task 1)  chatsNewMessages
apps/web/src/shell/Shell.tsx, Shell.module.css      modifies        (Task 2)  useMedia, .fill
apps/web/src/pages/Chats.tsx, Chats.module.css      modifies        (Task 2)  the two panes; (Task 4) OneToOne's column
apps/web/src/pages/Channel.tsx, Channel.module.css  modifies        (Task 3)  Feed, meetings fold, phone box
apps/web/src/pages/Channel.test.tsx                 tests           (Task 3)
apps/web/src/pages/OneToOne.tsx                     modifies        (Task 4)  Feed, paging, phone box
apps/web/src/pages/Chats.test.tsx                   tests           (Task 2, Task 4)
apps/web/e2e/chats-scroll.spec.ts                   creates, tests  (Task 5)
docs/SPEC.md, docs/design/web-ui.md                 modifies        (Task 6)
docs/design/mockups/ChatsPanes.dc.html, PhoneChatsPanes.dc.html  creates (with this plan)  the approved board
```

`Chats.module.css` is touched by Task 2 (the panes) and Task 4 (`.chat` as a column); `Chats.test.tsx` by Task 2 (the shell) and Task 4 (the one-to-one): each for its own rules.

## Interfaces

Consumes: `Button` (`@catervas/ui`), `useWide` (`Shell.tsx`), `channel.messages` and `chat.messages` with `before_seq` (`rpc.schema.json:6150`, `:6228`), `media` (`test/media.ts`), `renderApp`, `answerQuery`, `expectNoAxeViolations` (tests), `startServe`, `catervas`, `narrow` (e2e fixtures).

Produces:

```ts
// components/Feed.tsx
export function Feed(props: {
	labelledBy: string;                                  // the chat's h1 id
	newest: { seq: number; mine: boolean } | undefined;  // mine: the user wrote it
	oldest: number | undefined;                          // the oldest message's seq
	more: boolean;                                       // an earlier page exists
	paging: "busy" | "failed" | undefined;
	onEarlier: () => void;
	notes?: ReactNode;                                   // above the messages, inside the box
	children: ReactNode;                                 // the <ol>
}): ReactElement;
// shell/Shell.tsx
export function useMedia(query: string): boolean;
```

## Tasks

Unit tests in jsdom stub the box's geometry: `scrollHeight` and `clientHeight` on `HTMLElement.prototype` as getters returning values the test sets (restored after each test); `scrollTop` is jsdom's own stored value; a scroll is `fireEvent.scroll(box)` after setting `scrollTop`.

### Task 1: the feed

Files: `Feed.tsx`, `Feed.module.css`, `Feed.test.tsx`, `en.ts` (`chatsNewMessages: "New messages"`)

Tests (`Feed.test.tsx`, rendering `Feed` in a small harness whose rows and props the test changes; `box` is the region named by the harness's heading):

- `opens_at_the_newest` — scrollHeight 2000, clientHeight 500, ten rows: `box.scrollTop` is 2000.
- `follows_a_new_message_at_the_bottom` — opened as above; an eleventh row by someone else, scrollHeight 2100: `scrollTop` 2100, no "New messages" button.
- `stays_put_and_offers_the_newest_when_scrolled_up` — opened; `scrollTop` 300 and a scroll; an eleventh row by someone else, scrollHeight 2100: `scrollTop` stays 300 and "New messages" shows; pressed: `scrollTop` 2100, the button is gone, `document.activeElement` is `box`.
- `follows_your_own_message_even_when_scrolled_up` — scrolled up to 300; a row with `mine: true`, scrollHeight 2100: `scrollTop` 2100, no button.
- `clears_the_offer_at_the_bottom_by_hand` — the button showing at scrollHeight 2100; `scrollTop` 1600 (2100 − 1600 − 500 = 0) and a scroll: the button is gone; then `scrollTop` 1551 and a scroll (2100 − 1551 − 500 = 49): the button stays hidden; then another row by someone else, scrollHeight 2200: the button shows.
- `stays_at_the_bottom_when_the_box_changes` — opened at (2000, 500); rerendered with clientHeight 440 and scrollHeight 2060 and no new row: `scrollTop` 2060; then `scrollTop` 300 and a scroll, rerendered at scrollHeight 2100 and no new row: `scrollTop` stays 300.
- `asks_for_earlier_at_the_top` — `more`, not paging: a scroll at `scrollTop` 48 calls `onEarlier` once; at 49, not; with `paging` `"busy"` or `"failed"`, or `more` false, a scroll at 0 does not.
- `keeps_the_place_when_earlier_messages_arrive` — `scrollTop` 20 at scrollHeight 2000; `oldest` drops and scrollHeight becomes 3000: `scrollTop` 1020; no button.
- `offers_earlier_by_button_and_says_when_it_failed` — `more`: "Show earlier messages" is inside `box`, before the list, and pressing it calls `onEarlier`; `paging` `"busy"`: the button is busy; `"failed"`: the alert reads `channelEarlierFailed`; `more` false: no button.
- `passes_axe` — with the button "New messages" showing and "Show earlier messages" showing: no violations.

- [x] `feat(web): add a chat feed that opens at the newest message`

### Task 2: the shell gives the chats the window; two panes

Files: `Shell.tsx`, `Shell.module.css`, `Chats.module.css`, `Chats.test.tsx`
Produces: `useMedia`

Tests (`Chats.test.tsx`, importing `Shell.module.css` as `shell`):

- `fills_the_window_on_the_chats` — at `/channel` and at `/channel/mira`, the shell's root (the `main` element's parent) has `shell.fill`; at `/board`, it does not.
- `lists_the_chats` (unchanged) and every other test of the file still pass.

- [x] `fix(web): give the chats the window, the list and the chat side by side`

### Task 3: the channel in the feed, newest first

Files: `Channel.tsx`, `Channel.module.css`, `Channel.test.tsx`
Consumes: `Feed` (Task 1), `useMedia` (Task 2)

Tests (`Channel.test.tsx`):

- `opens_at_the_newest_message` — geometry stubbed (2000, 500), a page of 100: the region "Team" has `scrollTop` 2000.
- `loads_earlier_messages_at_the_top` — a page of 100 (seq 1001 to 1100): `scrollTop` 0 and a scroll on the region asks `channel.messages { before_seq: 1001, limit: 100 }`, once; answered with three, they are the list's first three rows.
- `shows_earlier_messages` and `says_when_earlier_messages_cannot_be_read` (unchanged: the button and the alert, now inside the region).
- `folds_the_meetings_below_1440` — a sprint with one meeting, `(min-width: 1440px)` false: the complementary "Meetings in this sprint" holds a closed `details` whose `summary` reads "Meetings in this sprint" and the meeting's link; set true: no `details`, an `h2` "Meetings in this sprint" and the link; `expectNoAxeViolations(container)` passes in both states.
- `opens_a_thread_from_the_side_panel` (unchanged, at the default: below 1440).
- `keeps_the_box_to_one_row_on_a_phone` — `(min-width: 1024px)` false: the paragraphs `#channel-enter` and `#channel-hint` have `styles.hidden` (the file's import of `Channel.module.css`); true: neither has it.

- [x] `fix(web): open the team chat at its newest message`

### Task 4: the one-to-one in the feed, with earlier pages

Files: `OneToOne.tsx`, `Chats.module.css`, `Chats.test.tsx`
Consumes: `Feed` (Task 1)

Tests (`Chats.test.tsx`):

- `opens_a_one_to_one_at_the_newest_message` — geometry stubbed (2000, 500), 20 messages: the region "Talking with Mira" has `scrollTop` 2000.
- `pages_back_in_a_one_to_one` — 100 messages (seq 201 to 300): "Show earlier messages" shows; pressed, `chat.messages { agent_id: "mira", before_seq: 201, limit: 100 }` is asked; answered with three, they are the first three rows and the button is gone.
- `asks_earlier_at_the_top_of_a_one_to_one` — 100 messages: `scrollTop` 0 and a scroll asks the same page, once.
- `keeps_more_after_a_live_message` — 100 messages; a live `chat_message.posted` makes the chat ask its newest page again, answered with 2 messages: the button still shows.
- `has_no_earlier_button_under_a_full_page` — 5 messages: no button.
- `says_when_an_earlier_chat_page_cannot_be_read` — the earlier page refused: the alert reads `channelEarlierFailed`.
- `keeps_the_chat_box_to_one_row_on_a_phone` — `(min-width: 1024px)` false: the label "Message Mira" and `#chat-enter` have `channel.hidden` (adding `import channel from "./Channel.module.css"`); true: neither has it.
- every other test of the file still passes.

- [x] `fix(web): open a one-to-one at its newest message and page back`

### Task 5: the browser proves it

Files: `apps/web/e2e/chats-scroll.spec.ts`

One test, `the chats open at the newest message, each pane scrolling on its own, and older ones load on scrolling up`, with `test.setTimeout(90_000)` (as `accept.spec.ts` does) and `{ exact: true }` on every "Line n." text: `startServe({ team: "pm-architect-developer", transcripts: ["reply_to_a_mention"], paceMs: 3000 })` (Theo's recorded reply waits three seconds before it plays); `catervas(project, ["say", "Line <n>."])` for n 1 to 130 (no `@`, so no agent answers); at 1280 by 800, Chats:

1. "Line 130." and the box "Post to the team" are in the viewport; `document.documentElement.scrollHeight <= innerHeight`; "Line 30." is not in the page (the first page is 31 to 130).
2. In one `evaluate` on the region "Team", `scrollTop = 0` is set and "Line 31."'s `getBoundingClientRect().top` returned (the scroll event fires after it); then "Line 30." is attached; "Line 31."'s top, read again, differs from the first by at most 4 px; the chat list's `nav` has `scrollTop` 0 and the same top as before.
3. `catervas say "@theo can you look at the menu page?"` (the user's own post, author `human`): it is in the viewport, the view having followed it, and no "New messages" shows. Then, in one `evaluate`, the region "Team" is scrolled to 0 (every page is read by now, so nothing more is asked). Theo's reply, "the login form is done and its tests are next.", arrives (timeout 15 s): the button "New messages" is visible and the reply is not in the viewport; pressed: the reply is in the viewport. (Amended during execution: a `catervas say` line is the user's own, which the view always follows, so the plan's first step 3 could never show the button; the executor stopped on it, as it should.)
4. `narrow(page)` (360 by 780), `/channel` again: Theo's reply and the box labelled "Message the team" (`channelPostPhone`, its label visually hidden) are in the viewport, and `document.documentElement.scrollHeight <= innerHeight`.

- [x] `test(web): open the chats at the newest message in a real browser`

### Task 6: the spec and the design say so

Files: `docs/SPEC.md` (0.103 header sentence; F8), `docs/design/web-ui.md` (Channel row). F8's new words, after "above the open chat": "The page fills the window and the list and the open chat scroll apart: the chat keeps its title at its top and its box at its bottom (on a phone the box's hints are read by screen readers alone). A chat opens at its newest message and follows new ones while the user is at its bottom; otherwise it stays where it is and shows \"New messages\", which goes to the newest, and the user's own message always goes there. Reaching the top reads the page before, as \"Show earlier messages\" above the first message does, the channel's and a one-to-one's alike, keeping what the user sees in place; a failed read stops until the button is pressed. Below 1440 pixels wide the Team chat's meetings fold under its intro (added in 0.103)."

Tests: none (docs).

- [x] `docs(spec): say the chats open at the newest message, the list apart`

## Verification

```
pnpm check
# expected: exit 0, every package's tests passed
pnpm --filter @catervas/web e2e chats-scroll chats channel
# expected: every journey passed
cargo xtask check
# expected: xtask check: ok (or, if this container lacks the room, the `check` workflow on the pull request)
```

Then the founder opens Chats with a long Team chat, lands on its newest message, scrolls up to read older ones, and sees the list stay where it was.
