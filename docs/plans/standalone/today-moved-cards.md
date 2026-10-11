# Standalone: what moved, as cards, newest first

Status: draft
Branch: `feat/today-moved-cards`
Spec: `docs/SPEC.md` section 5.7's first paragraph (line 340, Today's lists), section 8's web UI (Today); `docs/design/web-ui.md` (Today's row, line 121)
Depends on: none (`main` at `15d67ec`)
Readiness confirmed by: <pending>

## Goal

The founder, 2026-10-11: "Under what moved since yesterday in the main endpoint, show only the 3 most recent actions with a pagination on the rest. The also must be structured as a card with a picture of the the agent and a time stamp next to it. The current setup is a Queue that shows the most recent actions on the bottom which is not a good UX. The most recent actions must be on top." Then, on the first boards: "Instead of scrolling left and right make it a show more button that show more actions to the bottom. It should go from top to bottom." Approved design ("Approved.", same day; canvas https://claude.ai/artifact/RK9zWTa5mt3Gn2mrYAjFdq, kept as `docs/design/mockups/TodayMoved.dc.html`): Today's "What moved since yesterday" lists its moves newest first, three at a time; each is a card with the picture of who made the move, their name, their role tag when an agent, the time beside them, and the move's sentence under it; "Show more (<n> more)" under the cards adds the next three older ones below; once all show, and there were more than three, "Show fewer" goes back to the newest three. Out of scope: the sentences' wording (the founder took the defaults: kept as the store words them, the name in them too), the 24-hour window, times staying UTC as the team's lines say them.

## Decisions

- Who made a move comes from the daemon, not from parsing the sentence: `Moved` gains `by: String`, and `moved.since`'s rows gain `by`, required, one of an agent's id, `"human"` or `"catervas"`. Rejected: guessing the name from the line's first word (a task's title or "The" starts some lines, and a name can be two words).
- `by` for each kind of move: a task's move, its `requested_by`; `task.integrated`, `"catervas"` (Catervas adds it to the project); `human.accepted`, `"human"`; `agent.slept`, the envelope's agent id; a ceremony's post, its `author`; a post's `Wrote`, its agent; `Allowed`, `Stopped`, `Declined`, `"human"`; `Sent`, `Failed`, `Missed`, `PlanEnded`, `"catervas"` (Catervas tells of Buffer and of its own hand-over); every ads line, `"catervas"`. `"governor"` and any empty id become `"catervas"`, as `who` already names both "Catervas". Rejected: an absent `by` for the moves nobody on the team made (one more case for the page, for lines Catervas is the one telling).
- Order: the daemon keeps answering oldest first (the order a log reads; nothing else changes), and the page reverses it. Rejected: reversing in `moved_since`, which would reorder the store's three tests for no reader's gain.
- The card: `<li>` with a picture, then a head line (name in `strong`, `RoleTag` for an agent, `time`), then the sentence. An agent on the team: `Avatar` at 48 (the component's sizes are 32, 48 and 64; the board's 40 is not one), named by its display name. `"human"`: a person glyph tile, `aria-hidden`, named "You" (`movedYou`). `"catervas"`: `logo-mark-1254.png` at 48, `alt=""`, named by `t("brand")`, as `AdsRow.tsx:125` draws Catervas's own row. An agent id the team does not hold (retired and removed): its id as the name, no picture, no role tag. The picture's `alt` is empty where the name follows it, so a screen reader reads the name once.
- Time: today's `HH:MM`; an earlier date, "Yesterday HH:MM" (`movedYesterday`, "Yesterday {time}"), the list holding a day; both UTC from `at`, `time`'s `dateTime` the row's `at`. `yesterdayShort` ("Yday") and `yesterday` are deleted, Today being their only reader.
- Show more: `useState(3)`; the list shows the first `shown` of the reversed rows; while rows are hidden, a full-width `Button` "Show more ({n} more)" (`movedMore`) adds three; when all show and there are more than three, "Show fewer" (`movedFewer`) sets three again. Focus stays on the button pressed (the new cards land above it). Rejected: pages with Newer and Older (the founder's first boards, replaced), a "load more" from the daemon (a day's moves are already in hand).
- Style: `.moved` stays the bulletless grid; `.moved li` becomes the card (surface, `rule` border, `--catervas-radius-raised`, `space-3` `space-4` padding, flex with `space-3` gap); the head line wraps; `time` keeps the code face and `ink-muted`. The `Button` at the list's end takes the row's width.
- Spec revision 0.87 (2026-10-11), a header sentence at `docs/SPEC.md:3`, and one sentence at the end of 5.7's first paragraph (line 340): Today's "What moved since yesterday" shows the moves newest first as cards with who made each, three at first, "Show more" adding three; `moved.since` gives `by`. `docs/design/web-ui.md`'s Today row (line 121) says "What moved" is cards, newest first, with Show more.

## File map

```
crates/store/src/activity.rs                    modifies, tests  (Task 1)  Moved.by, each move's by
crates/runtime/src/daemon/gates.rs              modifies         (Task 1)  moved.since gives by
docs/schemas/rpc.schema.json                    modifies         (Task 1)  movedSinceResult's by, required
apps/web/src/pages/Today.tsx, Today.module.css  modifies         (Task 2)  the cards and Show more
apps/web/src/pages/Today.test.tsx               tests            (Task 2)
apps/web/src/strings/en.ts                      modifies         (Task 2)
docs/design/mockups/TodayMoved.dc.html          creates          (Task 3)  the approved board
docs/SPEC.md, docs/design/web-ui.md             modifies         (Task 3)
```

## Interfaces

Consumes: `moved_since`, `post_moves`, `ad_moves`, `name_of` (store); `Avatar`, `RoleTag`, `Button` (`@catervas/ui`); `Agent` (`TeamSetup.tsx`); `today`, `answerQuery`, `expectNoAxeViolations` (tests).

Produces:

```rust
pub struct Moved { pub at: DateTime<Utc>, pub by: String, pub line: String }
```
```json
"moved.since" → { "moved": [ { "at": "<date-time>", "by": "<agent id>|human|catervas", "line": "<sentence>" } ] }
```

## Tasks

### Task 1: each move says who made it

Files: `crates/store/src/activity.rs`, `crates/runtime/src/daemon/gates.rs`, `docs/schemas/rpc.schema.json`

Tests:

- `says_what_moved_since` asserts `(at, by, line)`: `linus` for Linus's move, `human` for the human's move and acceptances, `catervas` for both integrations (`integrated_by: governor`), `grace` for the nap, `mira` for the standup.
- `moved_tells_of_the_posts` asserts `by` per line: `kai` for each "Kai wrote", `human` for allowed, stopped and not allowed, `catervas` for handed, did not take, did not go out, and stopped for the plan's end.
- `says_what_moved_with_the_ads` asserts `by` is `catervas` on every line.
- The daemon's shape test (`gates.rs`, the `movedSinceResult` query near line 4072) holds the answer to the schema, which now requires `by`; a row with an empty agent id on `agent.slept` gives `catervas` (a new case in `says_what_moved_since`).

- [ ] `feat(store): say who made each move`

### Task 2: the cards, newest first, three at a time

Files: `Today.tsx`, `Today.module.css`, `Today.test.tsx`, `en.ts` (`movedYou`, `movedYesterday`, `movedMore`, `movedFewer` added; `yesterday`, `yesterdayShort` deleted)
Consumes: Task 1's `by`

Tests (`Today.test.tsx`; `today()`'s fixture team holds the agents named):

- `says_what_moved` (rewritten) — rows oldest first from the daemon, one by an agent today, one by `human`, one by `catervas` yesterday: the list's first card is the newest; an agent's card has its avatar image named by nothing extra (`alt=""`), its name, its role tag and `HH:MM` in a `time` whose `dateTime` is the row's `at`; the human's card reads "You"; Catervas's reads "Catervas" with the mark; yesterday's time reads "Yesterday HH:MM"; `since` is 24 hours back; axe passes.
- `shows_three_moves_then_more` — seven rows: three cards, newest first, and "Show more (4 more)"; pressed, six cards in order, the first three unchanged, "Show more (1 more)"; pressed, seven and "Show fewer", no Show more; pressed, three again; axe passes at each.
- `shows_no_button_for_three_or_fewer` — three rows: three cards, no Show more, no Show fewer.
- `names_an_agent_the_team_does_not_hold` — `by` an id not on the team: the card reads the id, with no image and no role tag.
- The empty case (`movedNone`) is unchanged.

- [ ] `feat(web): show what moved as cards, newest first, three at a time`

### Task 3: the spec and the design say so

Files: `docs/SPEC.md` (0.87 header sentence; 5.7's sentence), `docs/design/web-ui.md` (Today's row), `docs/design/mockups/TodayMoved.dc.html` (the approved canvas's `Main.dc.html`, light, with its four states as props)

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
