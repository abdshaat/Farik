# Standalone: fewer words on Settings

Status: ready
Branch: `fix/settings-fewer-words` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` section 6.7 (the "Service logos and info buttons" paragraph, 0.79, changed in 0.80), F15 (team rules in Settings); section 3's setup card for the preview (`docs/SPEC.md:101`, which names the fields by label and quotes no hint, so it stays)
Depends on: `InfoTip` (`@catervas/ui`, on main)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-10 (one round, against `docs/standards/workflow.md` stage 2)

## Goal

The founder, 2026-10-10: "There are too many words under Catervas settings. Under how to open your app, there is too much explanation under it. Just explain briefly and hide the details behind a hover-over information button. For example, the port field there is an explanation that this is the number your app answers on. This information should be hidden behind a hover-over information button. The explanation must be clear so that non technical users may understand what they do. For the rest of this project, any user facing explanation should be written in bullet points rather than paragraphs."

So on Settings: "How to open your app" shows two short bullets; its four fields show their labels, each with an ⓘ holding the detail; the other paragraphs Settings shows itself (Advanced, Your team's rules, Saved teams, Catervas on this computer) move behind an ⓘ beside their heading, as bullets. The setup card that shares the preview's fields gets the same fields and a bulleted lead. The bullet rule is written into the brand's voice so later work follows it.

Out of scope: the words inside the team-rules parts (What agents may do, spending, finishing work, planning, plans), which are setup's own question components (`PermissionChoices`, `IntegrationChoice`, `SprintsSwitch`, `PlanCheck`) and would change setup's pages too; offered to the founder as a follow-up. The theme choices, the account row and the language row, which are already one line each. Any refusal, status or button. The short muted lines that stay (`savedFolder`, `englishOnly`, the account's `accountKept`, advanced `uiPathsLead`) are one line or an instruction; offered as a follow-up if the founder wants them behind ⓘ too.

## Decisions

- `TextField` gains `info?: ReactNode`. When given, the label sits in a row (`.head`, flex, `align-items: center`, gap `--catervas-space-1`) with `<InfoTip id={`${id}-info`}>{info}</InfoTip>` after the `<label>`, outside it, so a click on the ⓘ does not focus the input. The input's `aria-describedby` lists `${id}-info` first (before hint and error), so a screen reader reads the detail on the input too; the tip stays in the DOM (InfoTip's contract). `hint` keeps working for every other caller.
- A bulleted explanation is one string in `en.ts`, one bullet per line (`\n`). `Bullets({ text }: { text: string })` in `apps/web/src/components/Bullets.tsx` renders `<ul className={styles.bullets}>` with one `<li>` per line (empty lines dropped); `Bullets.module.css`: `.bullets { margin: 0; padding-inline-start: 1.25em; max-width: 65ch }`. Rejected: one key per bullet (three keys for one thought), arrays in `en.ts` (`t` returns a string, and every caller would change). A `<ul>` inside InfoTip's tip `<span>` is block via `.tip > *`; InfoTip is never inside a `<p>` in this change.
- A heading with an ⓘ uses the existing `div.titled` pattern (`pages.module.css`, as `AgentEdit.tsx` and `Sites.tsx` do): `<div className={styles.titled}><h2 …/><InfoTip id="<key>-info"><Bullets text={t(key)} /></InfoTip></div>`.
- The words, plain for a non-technical reader. Keys are kept where the meaning stays, so the diff is the copy:

  | Key | Where | New words (each line a bullet) |
  |---|---|---|
  | `previewLead` | visible under "How to open your app" | `{designer} opens your app in a browser to check its screens.` / `Tell Catervas the commands that start it.` |
  | `previewMore` (new, replaces `previewNotSure`) | ⓘ beside "How to open your app" and beside the setup card's heading | `Catervas runs these commands inside Docker’s sandbox, a safe box kept apart from the rest of your computer.` / `Not sure what to type? Ask whoever set up your app, or look in its README file.` |
  | `previewSetupLead` | visible on setup's card | `{designer} opens your app in a browser to check its screens.` / `Tell Catervas the commands that start it.` / `Not sure? Leave these empty and add them in Settings later. {designer} waits until you do.` |
  | `previewPrepareHint` | ⓘ on "Get it ready (optional)" | `A command that installs what your app needs and builds it, such as “npm install”.` / `You can leave it empty.` / `It may use the internet, for up to 15 minutes.` |
  | `previewStartHint` | ⓘ on "Start it" | `The command that starts your app, such as “npm start”.` / `The internet is off while your app runs.` |
  | `previewPortHint` | ⓘ on "Port" | `The number your app answers on, like a door number.` / `Your app often shows it when it starts: in “localhost:3000” it is 3000.` / `It must be a number from 1024 to 65535.` (not `refusePreviewPort`'s exact words: `setup/team.test.tsx:822` finds that refusal by text and would match two elements) |
  | `previewPathHint` | ⓘ on "First page" | `The page {designer} opens first.` / `Leave it as “/” for your app’s home page.` |
  | `advancedBody` | visible, the Advanced switch's description | `Shows the detailed settings on each screen.` |
  | `advancedMore` (new, replaces `advancedSafe`) | ⓘ beside "Advanced settings" | `Catervas’s defaults are safe. You never need to change them.` / `Turning this on shows what each agent may do, the team’s rules, the checks, the models, and how a plan is judged ready.` / `Catervas refuses any change that would stop your team working, and says what to change instead.` |
  | `rulesNextSession` | ⓘ beside "Your team’s rules" | `Your answers from setup. Change them whenever you like.` / `A change starts with each agent’s next piece of work, never in the middle of one.` |
  | `savedTeamsLead` | ⓘ beside "Saved teams" (shown whenever the list is) | `Teams you saved from a Team page.` / `Any project on this computer can start from one.` / `Renaming or deleting one changes no project’s team.` |
  | `connectedBody` | ⓘ after "Connected." | `This page talks to Catervas on your computer.` / `Nothing leaves your computer except the requests to the AI models.` |

  `previewNotSure` and `advancedSafe` are deleted. "Connected." stays visible: it is a status. For `connectedBody` the ⓘ sits in the same `div.titled` as "Connected." (a `<p><strong>` today becomes `<div className={styles.titled}><strong>…</strong><InfoTip …/></div>`), never inside a `<p>`.
- `docs/brand/brand.md`'s Voice paragraph gains: "An explanation the user reads is short bullet points, never a paragraph: the screen shows a line or two, and the detail sits behind an info button (ⓘ)." (founder, 2026-10-10).
- Spec revision 0.84 (2026-10-10), one sentence in the header at `docs/SPEC.md:3` as every revision has: on Settings and on setup's preview card, field detail and section notes sit behind info buttons, and explanations are bullets. The 6.7 paragraph adds Settings' section notes and the preview's field details to what sits behind ⓘ, and says explanations are bullets, marked "changed in 0.84". `docs/SPEC.md:101` quotes no hint and is unchanged.

## File map

```
packages/ui/src/TextField.tsx, TextField.module.css   modifies: optional `info`
packages/ui/src/TextField.test.tsx                    tests:    the `info` prop
apps/web/src/components/Bullets.tsx, Bullets.module.css   creates
apps/web/src/components/Bullets.test.tsx              tests:    one item per line
apps/web/src/strings/en.ts                            modifies: the keys above (Tasks 3, 4)
apps/web/src/pages/setup/PreviewFields.tsx            modifies: `info` in place of `hint` (Task 3)
apps/web/src/pages/TeamRules.tsx                      modifies: Preview's lead and ⓘ (Task 3), rules' ⓘ (Task 4)
apps/web/src/pages/setup/SetupTeam.tsx                modifies: the setup card's lead and ⓘ (Task 3)
apps/web/src/pages/Settings.tsx                       modifies: Advanced, This computer (Task 4)
apps/web/src/pages/SavedTeams.tsx                     modifies: its lead (Task 4)
apps/web/src/pages/team.test.tsx                      tests:    Settings (Tasks 3, 4)
apps/web/src/pages/setup/team.test.tsx                tests:    the setup card (Task 3)
docs/brand/brand.md, docs/SPEC.md                     modifies: Task 4
```

## Interfaces

Consumes: `InfoTip`, `TextField` (`@catervas/ui`); `t`, `en` (`apps/web/src/strings`); `styles.titled` (`pages.module.css`); `onlyInTips` (`team.test.tsx:1707`, moved in Task 3 to `apps/web/src/test/tips.ts` so `setup/team.test.tsx` can use it).

Produces:

```ts
TextField: props & { info?: ReactNode }   // InfoTip `${id}-info` beside the label; input described by it
Bullets({ text }: { text: string }): JSX.Element   // <ul>, one <li> per non-empty line
```

## Tasks

### Task 1: `info` on TextField

Files: modified `packages/ui/src/TextField.tsx`, `TextField.module.css`, `TextField.test.tsx`

Tests:

- `it('puts its info behind a button beside the label')` — with `info="Detail."`: a button "More about this" is present, the label's element does not contain it, the element `#${id}-info` has `role="tooltip"` and text "Detail.", the textbox's accessible name is exactly the label, and its `aria-describedby` contains `${id}-info`; no axe violations.
- `it('has no info button without info')` — no button is rendered.

- [x] `feat(ui): let a text field carry an info button`

### Task 2: Bullets

Files: created `apps/web/src/components/Bullets.tsx`, `Bullets.module.css`, `Bullets.test.tsx`

Tests:

- `it('shows each line as a bullet')` — `text="One.\nTwo."` renders one `list` with two `listitem`s, "One." and "Two.", in order; `"One.\n\nTwo.\n"` renders the same two.

- [x] `feat(web): show an explanation as bullets`

### Task 3: How to open your app

Files: modified `en.ts` (the `preview*` keys), `PreviewFields.tsx`, `TeamRules.tsx` (`Preview`), `SetupTeam.tsx`, `team.test.tsx`, `setup/team.test.tsx`; created `apps/web/src/test/tips.ts` (`onlyInTips`, moved from `team.test.tsx`)
Consumes: Tasks 1, 2

`SetupTeam.tsx` styles from `setup.module.css`, which has no `.titled`: it imports `pages.module.css` for that class. The setup card's ⓘ id is `${id}-more-info` from its `preview-${index}` (two Designers give two cards); Settings' is `previewMore-info`.

Tests:

- `saves_the_preview_in_settings` (`team.test.tsx`) changes: the region holds `previewLead`'s two lines as list items (filled with "Iris"); `previewMore`'s lines are found only inside tooltips (`onlyInTips`); each of the four textboxes (found by its exact label, as today) is described by an element whose text contains its field's first line; every line of the four hint keys is only in tooltips; axe still passes. The rest of the test stays.
- The setup card's test in `setup/team.test.tsx` (the region "Iris needs to know how to open your app"): `previewSetupLead`'s three lines are list items in it; `previewMore`'s and the four hint keys' lines are only in tooltips; the port refusal later in the test is still found once by `en.refusePreviewPort`.

- [x] `fix(web): say briefly how to open your app, the detail behind info buttons`

### Task 4: the rest of Settings

Files: modified `en.ts` (`advancedBody`, `advancedMore`, `rulesNextSession`, `savedTeamsLead`, `connectedBody`), `Settings.tsx`, `TeamRules.tsx` (`TeamRules`), `SavedTeams.tsx`, `team.test.tsx`, `docs/brand/brand.md`, `docs/SPEC.md`
Consumes: Task 2 (`Bullets`); Task 3's `onlyInTips` move

Tests:

- `it('keeps Settings to short lines, the rest behind info buttons')` (`team.test.tsx`, beside the info-button tests) — on Settings with a saved team listed: each line of `advancedMore`, `rulesNextSession`, `savedTeamsLead` and `connectedBody` is only in tooltips; the Advanced switch's description is `advancedBody`; "Connected." is visible outside any tooltip; axe passes. (The saved-team listing is answered as the test at `team.test.tsx:1631` does; `settings()` does not answer `template.list`.)
- `changes_what_agents_may_do_and_the_limit_from_settings`'s first assertion becomes `onlyInTips` over `rulesNextSession`'s lines.

- [ ] `fix(web): move Settings' notes behind info buttons, as bullets`

## Verification

```
pnpm check
# expected: exit 0, every package's tests passed
cargo xtask check --integration
# expected: xtask check: ok (the e2e narrow() checks included)
```

Then the founder opens Settings with a UI/UX Designer on the team, at desktop and phone width, and hovers each ⓘ.
