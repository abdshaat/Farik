# Standalone UX pass, plan 8: SPEC revision 0.123 and the design notes

Status: ready
Branch: `feat/ux-pass` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.2, 6.7, F3, F17 (4.1 and 4.4 checked); revision 0.123
Depends on: plans 1 to 7 of this folder (committed on this branch)
Readiness confirmed by: the planner itself, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`docs/SPEC.md` and the design notes say what plans 1 to 7 changed (hard rule 8), in one revision, 0.123. Revision 0.122 is `fix/open-issues`'s (ADR 0069); whichever branch merges second resolves a one-line conflict at the top of SPEC.md (and in `en.ts`). No behaviour changes in this plan.

## Decisions

- One revision line, inserted directly after the line that starts "Revision 0.122" if this branch has one when the task runs, else after the line that starts "Revision 0.121". Its words:

  > Revision 0.123 (2026-10-17) records a pass over the web app's layout and words (standalone plan `ux-pass`, `docs/design/ux-pass-2026-10.md`): in 4.2 an agent waiting on the owner says only "Waiting on you" on Today's band and its Team card, "Waiting on you" groups its rows under "Needs setting up", "Needs your answer" and "Ready for you to merge" with three of the last shown at first, a question's or stuck task's row names its task and shows three lines of the agent's words, "What moved" comes before "Suggested connections", and a suggested answer to a task's question sends with one press; in 6.7 two suggested connections show at first; in F3 a task's page leads with the agent's note and its checks in one line, shows "Adding it to your project" only once there is something to add, and a board card's id and epic share one line while Done shows what waits on the owner and the five most recent; in F17 the Costs page lists each agent's spend highest first, with chats as the last row, the four numbers as tiles and their notes behind info buttons; and setup's team step and the Team page say less; no new event, command or query.

- 4.2: a new paragraph after "**The agents open their pages (added in 0.107).**":

  > **What waits, in groups (added in 0.123; standalone plan `ux-pass`).** "Waiting on you (n)" counts every row as before and shows them in up to three groups, each with its own heading and count and drawn only when it has a row: "Needs setting up" (the AI account's key, how to open the app, Docker's sandbox, the Designer's browser), then "Needs your answer" (questions, stuck tasks, connector calls, posts, orders, data sources, marketing plans and their budgets, document changes), then "Ready for you to merge" (finished work whose pull request waits), which shows three rows and "Show all n". A question's or a stuck task's row says "About “<task title>”" and shows at most three lines of the agent's words; the page its Answer opens shows them all. On the team band, and on the agent's card on the Team page, an agent waiting on the owner says only "Waiting on you"; every band line is at most two lines tall, the whole line in its tooltip. After "Waiting on you" come the seller mail, the renewals, the posts going out, "What moved", then "Suggested connections" (6.7).

- 4.2's "**A new project's Today …**" paragraph quotes the daemon's line "Waiting on you: your product plan", which stays true of `team.activity`; after that sentence add: "The web app shows a waiting agent's line as "Waiting on you" alone (changed in 0.123)."
- 4.2, after the paragraph above (a task's question page belongs to the working loop, not to 4.3's one-to-one chats):

  > **A question in one press (added in 0.123).** A task's question page shows the agent's question in its own paragraphs and each answer the agent suggests as a button that sends that answer at once, its hint under it; "Or answer in your own words" has its own box and "Send answer"; "Let <Name> decide" sends the agent's call. Why a question is no problem sits behind the info button beside the title.

- 6.7, in "**Suggested connections on Today (added in 0.105 …)**": "Today lists, under Waiting on you, the services" becomes "Today lists, after What moved (changed in 0.123), the services"; "Four rows show at first" becomes "Two rows show at first (changed in 0.123)".
- F3, at the end of its paragraph, one sentence: "On a task's page (changed in 0.123) the Summary tab leads with the agent's latest note, then what the task is for, then its checks as one line ("2 of 3 checks passed.") with each check under "Show each check"; the try count shows from the second try; "Adding it to your project" shows once the task is accepted, waits to be added, was added, or works in a private folder. A board card shows its title in at most three lines and its id and epic on one line; Done shows every task waiting on the owner and the five with the highest numbers, then "Show all n"."
- F17, at the end of its paragraph, one sentence: "The web app's Costs page (changed in 0.123) lists each agent's spend today, highest first, with the owner's chats as the last row, and shows the four rates as tiles, each note behind an info button, with active weeks and the message counts under "More numbers"."
- 4.1 and 4.4 quote none of the changed words ("We suggest six", "Start from the team Catervas suggests", "Pausing stops an agent", "What Catervas checks", "Summary and checks", "Right now": checked with grep on 2026-10-17), so they do not change.
- `docs/design/web-ui.md`: the Today row of the screens table (line 121) becomes "The team band, the request box, "Waiting on you" in groups (set up, answer, merge), "What moved" (cards, newest first, with Show more), then suggested connections."; the "A question from an agent" row becomes "The question with its avatar, each suggested answer as a button that sends it, and a free-text answer."
- `docs/design/suggested-connections.md` items 1 and 3: "under "Waiting on you" and above the seller mail, renewals and the posts going out" becomes "after "What moved" (changed by the UX pass, 2026-10-17)"; "At most four rows show at first." becomes "At most two rows show at first (four until the UX pass, 2026-10-17)."
- `docs/design/ux-pass-2026-10.md`'s Status line gains "; built, SPEC 0.123".

## File map

```
docs/SPEC.md                              modifies: revision 0.123; 4.2 two paragraphs; 6.7; F3; F17
docs/design/web-ui.md                     modifies: the Today and question rows of the screens table
docs/design/suggested-connections.md      modifies: items 1 and 3
docs/design/ux-pass-2026-10.md            modifies: status
docs/plans/standalone/ux-pass/README.md   modifies: every plan's checkbox ticked
```

## Interfaces

Consumes: what plans 1 to 7 committed. Produces: nothing in code.

## Tasks

### Task 1: SPEC 0.123

Files: modified the five above
Consumes: plans 1 to 7

Tests: none of code (a documentation change). The check:

- `grep -c "Revision 0.123" docs/SPEC.md` is 1; `grep -n "Four rows show at first" docs/SPEC.md` prints nothing; `grep -n "added in 0.123" docs/SPEC.md` prints the two 4.2 paragraphs.

- [x] `docs(spec): the UX pass, revision 0.123`

## Verification

```
grep -c "Revision 0.123" docs/SPEC.md
# expected: 1
grep -n "Four rows show at first" docs/SPEC.md
# expected: no output
cargo xtask check
# expected: xtask check: ok
```
