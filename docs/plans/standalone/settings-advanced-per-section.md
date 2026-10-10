# Standalone: advanced settings under each section

Status: ready
Branch: `fix/settings-fewer-words` (pull request #35, after `settings-directory.md`'s tasks)
Spec: `docs/SPEC.md` section 8's web UI paragraph (line 934), section 10's configuring requirement (line 959), F9 (the agent page), F15
Depends on: `docs/plans/standalone/settings-directory.md` (Tasks 1 to 4 and the review's fixes, on this branch)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-10 (one round, against `docs/standards/workflow.md` stage 2)

## Goal

The founder, 2026-10-10: "The advanced settings button is still on the general. By pressing it will show advanced settings on the rest of the pages which is a bad design. Make the show advanced settings button to be a pressable button under each category rather than a global button that open every advanced setting. Also the show advanced settings should not be a switch, it should be a pressable button that will open a list under it for every advanced setting in the category in which the button was pressed." Approved design (in chat, same day): one shared "Show advanced settings" button; pressed, it opens that category's advanced settings in a list right under it and reads "Hide advanced settings"; only where a category has advanced settings; each opens only its own, nothing is remembered or shared, every page starts closed. General loses the switch; How work runs gets the button over its UI file patterns; the agent page's "What it may do" switch becomes the button over the permission switches and "Add your own connector" (the model menu shows model ids while it is open); the plan editor's switch becomes the button, its advanced fields staying in place. Out of scope: setup's advanced step and the setup-finish switch that leads to it (`SetupFinish.tsx:90`), a page of its own rather than a reveal.

## Decisions

- `@catervas/ui`'s `Button` (`packages/ui/src/Button.tsx`) passes neither `aria-expanded` nor `aria-controls` today, so it gains two optional props, `expanded?: boolean` and `controls?: string`, rendered as `aria-expanded` and `aria-controls` and left off when not given. Rejected: a plain `<button>` in the app (as `Gate.tsx:271`), which would not look like the quiet `Button` beside it.
- `ShowAdvanced({ open, onToggle, controls }: { open: boolean; onToggle: () => void; controls?: string })` in `apps/web/src/components/ShowAdvanced.tsx`: `Button` with `kind="quiet"`, `expanded={open}`, `controls={controls}`, its text `advancedSwitch` ("Show advanced settings") closed and `advancedHide` open (`en.ts:322`, "Hide advanced settings", already there: setup's advanced step's way back uses it, `SetupAdvanced.tsx:276`). axe accepts an `aria-controls` whose element is absent while `aria-expanded="false"` (axe-core 4.13's `aria-valid-attr-value` skips that case), so the list need not exist while closed. The caller holds `useState(false)`. Rejected: `<details>` (the founder asked for a button; its summary is read as a disclosure, not a button, and its words cannot change), a `Switch` (asked against), an uncontrolled wrapper (the plan editor reveals fields in place, so the state must sit with the page).
- Where a category's advanced settings form a list, the caller renders, after the button and only while open, `<ul id={controls} className={styles.tiers} aria-label={t("advanced")}>` (`pages.module.css`'s bulletless grid, today's tier list's) with one `<li>` per advanced setting. `t("advanced")` is "Advanced settings" (`en.ts:75`).
- Settings › General: the Advanced section (its `h3`, the `Switch`, `advancedBody`, `advancedMore`'s ⓘ) is removed, and its phone summary `sectionGeneralSummary` (`en.ts:17`) becomes "Theme and language".
- Settings › How work runs: `Work.tsx` holds `useState(false)`; after the plans part, `ShowAdvanced controls="work-advanced"`; open, the list holds one item, `UiPaths`. No other Settings section renders the button (none has advanced settings).
- Agent page (`AgentEdit.tsx`): `useState(false)` in place of `useAdvanced()`. In "What it may do", `ShowAdvanced controls="agent-advanced"` replaces the `Switch` `agent-advanced`; open, the list holds the permission tiers (each `li` the tier's `Switch`, as today's `ul.tiers`, which this list replaces: its name becomes "Advanced settings", no longer "What <name> may do") and an `li` with the custom-connector heading, its ⓘ and "Add your own connector". `agentMayAdvanced`'s ⓘ is removed. The model menu shows `label (id)` while open, `label` closed, as today with the flag.
- Plan editor (`PlanEditor.tsx`): `useState(false)` in place of `useAdvanced()`; `ShowAdvanced` (no `controls`) replaces the `Switch` `plan-advanced`, where it stands; every place that read `advanced` reads the page's state, unchanged.
- `apps/web/src/app/advanced.ts` (`useAdvanced`, the `catervas.advanced` key) is deleted. A browser that still holds the key ignores it.
- Words: none added; `sectionGeneralSummary` changed; `advancedBody`, `advancedMore`, `agentMayAdvanced`, `editAdvanced` deleted. `advanced`, `advancedSwitch` and `advancedHide` stay (setup's advanced step and the setup-finish switch use them too).
- Spec revision 0.86 (2026-10-10), a header sentence at `docs/SPEC.md:3`: advanced settings open under their own section with a "Show advanced settings" button, not one switch for every page. The SPEC sentences that change: line 934 (General "the theme, the Advanced switch and the language" loses the switch; How work runs "with Advanced on, the UI file patterns" becomes "the UI file patterns under Show advanced settings") and line 680 ("the notes under Advanced settings" on Settings goes, those notes being deleted). Lines 35, 101, 434, 782, 852, 910 (a setting's or the agent page's advanced view, setup's switch) stay true and stay. `docs/design/settings-directory.md`: General's row (line 19) and the summary "Theme, language, advanced" (line 35) drop advanced, How work runs' row (line 22) says the patterns sit under its button, and line 28's sentence on the Advanced switch staying in General is replaced by one saying each section opens its own. `docs/design/web-ui.md:130`'s Settings row drops "the Advanced switch". ADRs are history and are not edited.

## File map

```
packages/ui/src/Button.tsx, Button.test.tsx                   modifies, tests  (Task 1)
apps/web/src/components/ShowAdvanced.tsx, ShowAdvanced.test.tsx   creates          (Task 1)
apps/web/src/pages/settings/General.tsx, Work.tsx              modifies         (Task 1)
apps/web/src/pages/settings/settings.test.tsx                  tests            (Task 1)
apps/web/src/pages/team.test.tsx, pages.test.tsx               tests            (Tasks 1, 2: team.test.tsx)
apps/web/src/pages/AgentEdit.tsx                               modifies         (Task 2)
apps/web/src/pages/connectors.test.tsx                         tests            (Task 2)
apps/web/src/pages/PlanEditor.tsx, PlanEditor.test.tsx         modifies, tests  (Task 3)
apps/web/src/app/advanced.ts                                   deleted          (Task 3)
apps/web/src/strings/en.ts                                     modifies         (Tasks 1, 2, 3)
docs/SPEC.md, docs/design/settings-directory.md, docs/design/web-ui.md   modifies (Task 4)
```

## Interfaces

Consumes: `Button` (`@catervas/ui`, with Task 1's two props), `t`, `en`; `UiPaths`, `useRuleParts` (`TeamRules.tsx`, this branch); `onlyInTips`, `renderApp`, `answerStatus`, `answerQuery` (tests).

Produces:

```ts
// packages/ui/src/Button.tsx, two props added to the existing ones
expanded?: boolean; // aria-expanded
controls?: string;  // aria-controls
// apps/web/src/components/ShowAdvanced.tsx
export function ShowAdvanced(props: { open: boolean; onToggle: () => void; controls?: string }): JSX.Element;
```

## Tasks

### Task 1: the button, and Settings' only advanced settings under it

Files: modified `packages/ui/src/Button.tsx`, `Button.test.tsx`; created `ShowAdvanced.tsx`, `ShowAdvanced.test.tsx`; modified `General.tsx`, `Work.tsx`, `en.ts` (`sectionGeneralSummary` changed; `advancedBody`, `advancedMore` deleted), `settings.test.tsx`, `apps/web/src/pages/team.test.tsx`, `apps/web/src/pages/pages.test.tsx`

Tests:

- `Button.test.tsx`, `it('says whether what it opens is open')` — `expanded` and `controls` given: the button has `aria-expanded` and `aria-controls` with those values; neither given: it has neither attribute; axe passes.
- `ShowAdvanced.test.tsx`, `it('says show or hide and whether it is open')` — closed: a `button` named "Show advanced settings", `aria-expanded="false"`; open: named "Hide advanced settings", `aria-expanded="true"`, `aria-controls` the given id; a click calls `onToggle` once; no `switch` role; axe passes.
- `settings.test.tsx`, `opens_work's_advanced_settings_under_its_button` — `/settings/work`: no `UiPaths` region (`uiPathsTitle`) and the button reads "Show advanced settings"; pressed, a list named `advanced` holds the `UiPaths` region and the button reads "Hide advanced settings"; pressed again, the list is gone. Axe passes open and closed.
- `settings.test.tsx`, `keeps_advanced_settings_to_their_own_section` — `localStorage` holding `catervas.advanced=true`: `/settings/work` still opens closed; General, Allowed actions, Spending, Your app, Saved teams, This computer and AI account have no button named "Show advanced settings" and no `switch` named `advancedSwitch`.
- The existing tests that turned Advanced on for Work (`edits_the_ui_paths_in_advanced`, `team.test.tsx:745`; the end of `splits_the_rules_across_three_sections`, `settings.test.tsx:269`) press the button instead of setting `catervas.advanced`; the General tests, renamed `settings_changes_the_theme` (`pages.test.tsx:27`) and `holds_the_theme_and_the_language_in_general` (`settings.test.tsx:182`), assert no `switch` named `advancedSwitch`; the "short lines" test (`team.test.tsx:1834`) drops `advancedMore` and the switch's `advancedBody` description.

- [x] `feat(web): open advanced settings under their own section`

### Task 2: the agent page's advanced settings under its button

Files: modified `AgentEdit.tsx`, `en.ts` (`agentMayAdvanced` deleted), `apps/web/src/pages/team.test.tsx`, `connectors.test.tsx`
Consumes: Task 1

Tests:

- `opens_the_agent's_advanced_settings_under_its_button` (`team.test.tsx`) — an agent's page: no tier switches, no "Add your own connector", model options read `label` only; pressed, a list named `advanced` holds every tier's switch and the custom-connector button, and the model options read `label (id)`; the button reads "Hide advanced settings"; axe passes.
- `starts_closed_on_each_page` (`team.test.tsx`) — open Work's advanced settings at `/settings/work`, then go to an agent's page: its list is closed (no shared state).
- Every existing test that clicked the `switch` named `advancedSwitch` on the agent page (`team.test.tsx:527` and `:1768`; `connectors.test.tsx:129, 289, 395, 492, 548, 576, 612, 1053, 2724`) presses the button instead; `toggles_tiers_in_advanced` finds the tiers in the list named "Advanced settings", not "What Ada may do"; the info-button test (`team.test.tsx:1760`) drops `agentMayAdvanced`. `setup/team.test.tsx`'s clicks on the setup-finish switch (`:1085, 1341, 1371`) are setup's and stay.

- [x] `feat(web): open an agent's advanced settings under their own button`

### Task 3: the plan editor's button, and no global switch left

Files: modified `PlanEditor.tsx`, `PlanEditor.test.tsx`, `en.ts` (`editAdvanced` deleted); deleted `apps/web/src/app/advanced.ts`
Consumes: Task 1

Tests:

- `shows_the_plan's_advanced_fields_in_place_when_asked` (`PlanEditor.test.tsx`) — closed: a criterion's "Who checks it" choice has no command option and the risk choice is absent; pressed, the command option and the risk choice appear where they were, and the button reads "Hide advanced settings"; no `switch` named `editAdvanced`.
- Any existing plan-editor test that turned `plan-advanced` on presses the button instead.
- No file under `apps/web/src` names `useAdvanced`, and only `keeps_advanced_settings_to_their_own_section` names `catervas.advanced` (`grep -rn "useAdvanced\|catervas.advanced" apps/web/src` prints that one line).

- [ ] `feat(web): open a plan's advanced fields with a button`

### Task 4: the spec and the design say so

Files: modified `docs/SPEC.md` (0.86 header sentence; the Advanced sentences), `docs/design/settings-directory.md`, `docs/design/web-ui.md`

Tests: none (docs).

- [ ] `docs(spec): advanced settings open under their own section`

## Verification

```
cargo xtask check
# expected: xtask check: ok
pnpm check
# expected: exit 0, every package's tests passed
pnpm --filter @catervas/web e2e
# expected: 15 passed (no journey uses the Advanced switch; Rust is unchanged)
```

Then the founder opens How work runs and an agent's page, presses "Show advanced settings" on each, and checks that one does not open the other.
