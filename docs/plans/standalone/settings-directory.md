# Standalone: Settings as a directory

Status: ready
Branch: `fix/settings-fewer-words` (pull request #35, after `settings-fewer-words.md`'s tasks)
Spec: `docs/SPEC.md` section 8's web UI paragraph (line 934), section 4's Change project (line 131), section 10's configuring requirement (line 959), F15
Design: `docs/design/settings-directory.md` and its mockup (https://claude.ai/artifact/FWts6N4gxRahTurXACVT9W), both approved by the founder on 2026-10-10
Depends on: `docs/plans/standalone/settings-fewer-words.md` (Tasks 1 to 5, on this branch: `Bullets`, `TextField`'s `info`, `onlyInTips`)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-10 (one round, against `docs/standards/workflow.md` stage 2)

## Goal

Settings is a directory: a side list of eight sections on a desktop, a list page on a phone, each section at its own address under `/settings/`, holding the settings it holds today and nothing new. Every link into Settings lands on its section. Out of scope: rewording the team-rules questions, any setting's behaviour, the agent pages under Team, search.

## Decisions

- Routes: `App.tsx` nests `/settings` → `SettingsLayout` (an `Outlet`), with an index route and one child per section: `general`, `allowed-actions`, `spending`, `work`, `your-app`, `saved-teams`, `computer`, `account`; any other child falls to the shell's `*` not-found route (no `settings/*` catch-all). Rejected: one page switching on a `:section` param (one file keeps growing), hash anchors (still one page).
- `SettingsLayout` (`apps/web/src/pages/settings/SettingsLayout.tsx`) holds `SECTIONS`, the one list of the sections' order, paths, names and summaries. It renders the `h1` "Settings" and, on a wide screen (`useWide()`, `Shell.tsx`), a `nav` named by `settingsSections` ("Settings sections") with one `NavLink` per section (`aria-current="page"` from NavLink) beside the `Outlet`. On a phone the index shows the same `nav` alone, each row the name and its summary; a section shows a link to `/settings` above the `Outlet`, its text `settingsBack` ("Settings") after an `aria-hidden` "←". The layout renders the `h1` and the `Outlet` without waiting for its `team.get` (only the Your app link waits for it), so a section's own queries are asked in the same render as the layout's and a test's `answerQuery` answers both.
- The index (`SettingsIndex`, Task 1): `/settings` with hash `#preview` → `<Navigate to="/settings/your-app" replace />` on any screen; else on a wide screen → `<Navigate to="/settings/general" replace />`; else the list.
- "Your app" is in `SECTIONS` with `needsDesigner: true`: the layout reads `team.get` (as `HowToOpen` does) and leaves it out unless a non-retired `ui_ux_designer` is on the team. Its page without one shows `yourAppNone` ("Add a UI/UX Designer to your team first.") with a link to `/team` (`team` word): `HowToOpen` takes an optional `none: ReactNode`, shown instead of `null` once the team is read and has no non-retired Designer, so `YourApp` is `<HowToOpen none={…} />` and the page asks `team.get` once (a second, nested `team.get` would be asked after a test's `answerQuery` returned). `Preview`'s `#preview` scroll (`useLocation`, `useRef`, `useEffect`, `TeamRules.tsx:337-341`) is deleted in Task 1: the index's redirect drops the hash, so nothing reaches it.
- Headings: a section built from one existing block keeps that block's own `h2` as its page heading: Your app (`previewTitle`), Saved teams (`savedTeams`), This computer (`thisComputer`), AI account (`accountRow`). A section built from several parts (General, Allowed actions, Spending, How work runs) gets an `h2` with its name, and the parts beneath are `h3`: `TeamRules.tsx`'s `Part` becomes `h3`; General's Advanced and Language headings become `h3`; Theme stays `Choice`'s legend. Every region keeps its accessible name, which the e2e journeys find it by ("What agents may do", "Planning work", "How finished work is added", "Saved teams").
- The three rules pages each carry `rulesNextSession`'s ⓘ (`Bullets`) beside their `h2`, in `div.titled`; "Your team's rules" (`rulesArea`) is no longer a heading.
- `useRuleParts()` (in `TeamRules.tsx`) returns `ready`, `part(name)` (the props) and `key(name)` (today's keys: `` `${name}-${rounds[name] ?? 0}` ``, and for `spend` the saved `budgets.dailyUsd` appended, as `TeamRules.tsx:74` does; React does not take `key` through a spread): today's `TeamRules` body (the `team.get`, `useDefaults`, `team.validate` base check and the rounds) as a hook, so each rules page renders its parts with today's keys and saving one still restarts only that one. `May`, `Spend`, `Finish`, `Planning`, `Plans`, `UiPaths` are exported. `TeamRules` (the wrapper) is deleted in Task 2, and `rulesArea` with it. `Rules.tsx` (Task 1) and `Work.tsx` (Task 2) read `useAdvanced()` themselves for `UiPaths`; the switch is on General, and each page reads storage when it mounts.
- `Computer.tsx` is today's This computer section (`Settings.tsx:104-126`, its `serve.status`, Change project and Disconnect) with the `eventsForTesting` link at its foot; `General.tsx` is the theme `Choice`, the Advanced section and the Language section.
- The theme stays one state in `App.tsx`; the `general` route's element is `<General theme={theme} onTheme={setTheme} />`.
- `useAdvanced` moves from `Settings.tsx` to `apps/web/src/app/advanced.ts`, and `AgentEdit.tsx` and `PlanEditor.tsx` import it from there; `Settings.tsx` is deleted, its `Account` moving to `settings/Account.tsx`.
- During Task 1 the team's rules sit on one temporary section, `rules` ("Your team's rules", today's `TeamRules` at `/settings/rules`), so each commit keeps every setting reachable; Task 2 replaces it with the three pages. Rejected: dropping the rules for a commit.
- Layout CSS in `pages.module.css`: `.settings` a wrapping flex row, the `nav` `flex: 0 0 15rem`, the section `flex: 999 1 35rem; min-width: 0` (the side list stacks rather than squeezing); a current row is bold on `--catervas-color-surface` with a `--catervas-color-control-border` border, others transparent; rows at least 44 px tall. The phone list's rows are links at least 56 px tall, name over summary, a chevron `aria-hidden`.
- New words in `en.ts`: `settingsSections` "Settings sections"; `settingsBack` "Settings"; names `sectionGeneral` "General", `sectionAllowedActions` "Allowed actions", `sectionSpending` "Spending", `sectionWork` "How work runs", `sectionYourApp` "Your app", `sectionSavedTeams` "Saved teams", `sectionComputer` "This computer", `sectionAccount` "AI account", (Task 1 only) `sectionRules` "Your team’s rules"; summaries `sectionGeneralSummary` "Theme, language, advanced", `sectionAllowedActionsSummary` "What agents may do", `sectionSpendingSummary` "The daily limit", `sectionWorkSummary` "Sprints and plan checks", `sectionYourAppSummary` "How to open your app", `sectionSavedTeamsSummary` "Your saved teams", `sectionComputerSummary` "Folder and connection", `sectionAccountSummary` "Your Claude account", (Task 1 only) `sectionRulesSummary` "Spending, permissions and plans"; `yourAppNone` as above. `finishCarriedAfter` becomes "." (Task 3).
- Spec revision 0.85 (2026-10-10), a header sentence at `docs/SPEC.md:3`: Settings is a directory of eight sections at their own addresses, a side list on a desktop and a list on a phone. Section 8's web UI paragraph and section 4's Change project ("Settings, 'This computer'") name the addresses; section 10 is unchanged. `docs/design/web-ui.md`'s Settings row (line 130) lists the eight sections.

## File map

```
apps/web/src/app/advanced.ts                     creates: useAdvanced (moved)                     (Task 1)
apps/web/src/app/App.tsx                         modifies: nested settings routes                 (Tasks 1, 2)
apps/web/src/pages/settings/SettingsLayout.tsx   creates: SECTIONS, list, back link, index        (Tasks 1, 2)
apps/web/src/pages/settings/General.tsx          creates                                          (Task 1)
apps/web/src/pages/settings/YourApp.tsx          creates: HowToOpen or yourAppNone                (Task 1)
apps/web/src/pages/settings/SavedTeamsPage.tsx   creates                                          (Task 1)
apps/web/src/pages/settings/Computer.tsx         creates                                          (Task 1)
apps/web/src/pages/settings/Account.tsx          creates: Account (moved)                         (Task 1)
apps/web/src/pages/settings/Rules.tsx            creates (Task 1), deleted (Task 2)
apps/web/src/pages/settings/AllowedActions.tsx, Spending.tsx, Work.tsx   creates                  (Task 2)
apps/web/src/pages/settings/settings.test.tsx    tests: the directory                             (Tasks 1, 2, 3)
apps/web/src/pages/Settings.tsx                  deleted                                          (Task 1)
apps/web/src/pages/TeamRules.tsx                 modifies: exports, Part h3, useRuleParts; wrapper deleted (Tasks 1, 2)
apps/web/src/pages/AgentEdit.tsx, PlanEditor.tsx modifies: useAdvanced's import                   (Task 1)
apps/web/src/pages/pages.module.css              modifies: .settings and its list                 (Task 1)
apps/web/src/strings/en.ts                       modifies: the words above                        (Tasks 1, 2, 3)
apps/web/src/pages/team.test.tsx                 tests: Settings tests moved to their sections    (Tasks 1, 2)
apps/web/src/pages/pages.test.tsx                tests: theme, disconnect, Change project moved   (Task 1)
apps/web/src/shell/Shell.test.tsx                tests: the Events link's new place               (Task 1)
apps/web/src/pages/Today.tsx, OneToOne.tsx, Team.tsx, dialogs/UseTemplate.tsx, setup/SetupFinish.tsx   modifies: links (Task 3)
apps/web/src/pages/Today.test.tsx, Chats.test.tsx, team.test.tsx, setup tests   tests: the links  (Task 3)
apps/web/e2e/templates.spec.ts, board.spec.ts, sprints.spec.ts, setup-team.spec.ts   modifies: section addresses (Tasks 1, 2)
docs/SPEC.md, docs/design/web-ui.md, docs/design/settings-directory.md   modifies                 (Task 4)
```

## Interfaces

Consumes: `useWide` (`apps/web/src/shell/Shell.tsx`), `HowToOpen`, `May`, `Spend`, `Finish`, `Planning`, `Plans`, `UiPaths` (`TeamRules.tsx`), `SavedTeams`, `ChangeProject`, `ConnectAgain`, `Bullets`, `InfoTip`, `onlyInTips` (`apps/web/src/test/tips.ts`), `media` (`apps/web/src/test/media.ts`; `WIDE` is a local constant, as `Shell.test.tsx:20` has it), `renderApp`, `answerStatus`, `answerQuery`.

Produces:

```ts
// apps/web/src/app/advanced.ts
export function useAdvanced(): [boolean, (on: boolean) => void];
// apps/web/src/pages/settings/SettingsLayout.tsx
type Section = { path: string; name: keyof typeof en; summary: keyof typeof en; needsDesigner?: true };
export const SECTIONS: readonly Section[];
export function SettingsLayout(): JSX.Element;
export function SettingsIndex(): JSX.Element;
// apps/web/src/pages/TeamRules.tsx
export function useRuleParts(): { ready: boolean; part: (name: string) => Part; key: (name: string) => string };
export function HowToOpen(props: { none?: ReactNode }): JSX.Element | null;
// apps/web/src/pages/settings/General.tsx
export function General(props: { theme: ThemeChoice; onTheme: (c: ThemeChoice) => void }): JSX.Element;
```

## Tasks

### Task 1: the directory, with the rules on one page for now

Files: created `advanced.ts`, `SettingsLayout.tsx`, `General.tsx`, `YourApp.tsx`, `SavedTeamsPage.tsx`, `Computer.tsx`, `Account.tsx`, `Rules.tsx`, `settings.test.tsx`; modified `App.tsx`, `TeamRules.tsx` (exports, `Part` as `h3`, `HowToOpen`'s `none`, `Preview`'s scroll deleted), `AgentEdit.tsx`, `PlanEditor.tsx`, `pages.module.css`, `en.ts`, `team.test.tsx`, `pages.test.tsx`, `Shell.test.tsx`, the four e2e specs; deleted `Settings.tsx`
Consumes: the items above

Tests (`settings.test.tsx` unless named):

- `opens_general_beside_the_list_on_a_wide_screen` — `media.set(WIDE, true)`, `/settings`: the heading `sectionGeneral` shows, the `nav` named `settingsSections` holds links in `SECTIONS`' order (with a Designer on the team, nine in Task 1 counting the rules), the General link has `aria-current="page"` and no other in that `nav` has it, there is no `settingsBack` link within `main` (the rail's own "Settings" link has the same name, so the query is scoped); axe passes.
- `shows_the_list_alone_on_a_phone` — default media, `/settings`: the `nav` holds each section's name and summary; no `sectionGeneral` heading; axe passes.
- `puts_a_way_back_above_a_section_on_a_phone` — `/settings/general` on a phone: a link named `settingsBack` with `href` `/settings` precedes the `h2`; on a wide screen it is absent from `main`.
- `leaves_your_app_out_without_a_designer` — the team without a `ui_ux_designer`: no `sectionYourApp` link; `/settings/your-app` shows `yourAppNone` and a link to `/team`. With one (retired): the same. With one not retired: the link is there and the page holds the region `previewTitle`.
- `shows_not_found_for_an_unknown_section` — `/settings/nope` shows the not-found page's words.
- `takes_an_old_preview_link_to_your_app` — `/settings#preview` on a phone and on a wide screen shows the region `previewTitle` at `/settings/your-app` (the Your app link `aria-current` on wide).
- General: the theme choice, `english`, and the switch `advancedSwitch` with `advancedMore`'s lines only in tips, all on `/settings/general`.
- `pages.test.tsx`: `settings_changes_the_theme_and_disconnects` splits: theme and Advanced on `/settings/general` (one `answerStatus`, the shell's), the folder, address, axe and Disconnect on `/settings/computer` (`answerStatus(s, false, 2)` as today); the two Change project tests go to `/settings/computer` unchanged.
- The existing Settings tests in `team.test.tsx` move to their section's address with their assertions: the preview (`saves_the_preview_in_settings`) to `/settings/your-app`, its `scrollIntoView` stub and assertion deleted (the scroll is gone; `takes_an_old_preview_link_to_your_app` holds the old link), and its no-Designer tail now expects `yourAppNone`; `edits_the_ui_paths_in_advanced` sets `localStorage` `catervas.advanced` to `"true"` before opening `/settings/rules` instead of clicking the switch (and its first assertion, no `uiPathsTitle` with Advanced off, opens the page once without it); the account tests (`says_a_failed_disconnect_in_plain_words`, `shows_and_disconnects_the_account`, `says_the_key_did_not_work_and_connects_again`, the two environment-variable tests) to `/settings/account`, the rules tests to `/settings/rules`, saved teams (`renames_and_deletes_in_settings`) to `/settings/saved-teams`; `keeps Settings to short lines…` checks each moved note on its own section (`advancedMore` on general, `rulesNextSession` on rules, `savedTeamsLead` on saved-teams, `connectedBody` and `connectedLead` on computer). The `settings()` helper takes the section's path and answers only the queries that section asks.
- `Shell.test.tsx`'s `shows_the_rail_on_a_wide_screen_and_a_bar_on_a_phone` finds `eventsForTesting` on `/settings/computer`.
- e2e: `templates.spec.ts` (lines 58, 172) and `board.spec.ts` (227), `sprints.spec.ts` (245) and `setup-team.spec.ts` (90) go to `/settings/rules` or `/settings/saved-teams`. `setup-project.spec.ts:78` keeps `/settings`: it checks that the shell sends a deep link back to setup, before any Settings route draws.

- [x] `feat(web): make Settings a directory of sections`

### Task 2: the team's rules on three pages

Files: created `AllowedActions.tsx`, `Spending.tsx`, `Work.tsx`; modified `TeamRules.tsx` (`useRuleParts`, wrapper deleted), `SettingsLayout.tsx` (the rules section out, the three in), `App.tsx`, `en.ts` (`sectionRules*` and `rulesArea` deleted), `settings.test.tsx`, `team.test.tsx`, the four e2e specs; deleted `Rules.tsx`
Consumes: Task 1

Tests:

- `splits_the_rules_across_three_sections` — `/settings/allowed-actions` holds the region `rulesMay` and no other part; `/settings/spending` holds `rulesSpendTitle` alone; `/settings/work` holds `rulesFinish`, `rulesPlanning` and the plan check, and `uiPathsTitle` only with Advanced on; each page's `h2` is its section name with `rulesNextSession`'s lines only in tips; axe passes on each.
- `opens_general_beside_the_list_on_a_wide_screen` now expects eight links (with a Designer), no `sectionRules`.
- The rules tests in `team.test.tsx` move from `/settings/rules` to their page; the `settings()` helper's `part(name)` looks within `main` (no `rulesArea` region is left) and it returns no `rules`; `switches_planning_in_sprints` uses `main` where it used `rules`. Saving one part still leaves another's draft (`keeps_each_rules_draft_until_it_is_saved_or_cancelled`, kept on `/settings/work` with two of that page's parts).
- e2e: `templates.spec.ts:58` → `/settings/allowed-actions`, `board.spec.ts:227`, `sprints.spec.ts:245` and `setup-team.spec.ts:90` → `/settings/work`.

- [x] `feat(web): put the team's rules on three Settings pages`

### Task 3: links land on their section

Files: modified `Today.tsx`, `OneToOne.tsx`, `Team.tsx`, `dialogs/UseTemplate.tsx`, `setup/SetupFinish.tsx`, `en.ts` (`finishCarriedAfter`), their tests
Consumes: Task 1 (`/settings/your-app`, `/settings/account`, `/settings/saved-teams`)

Tests:

- `Today.test.tsx`: the `preview_missing` row's link `href` is `/settings/your-app`; the refused-key row's is `/settings/account`.
- The chat's refused-key link (`Chats.test.tsx`, or wherever `chatsKeyLink` is tested) has `href` `/settings/account`.
- The Team page's saved notice and the Use-a-saved-team dialog link to `/settings/saved-teams` (`team.test.tsx`: the test asserting `savedAsAfter` at line 1434, and the test that opens the Use-a-saved-team dialog, which gains the assertion, `useSettingsBefore` being untested today).
- Setup's finish screen (`setup/team.test.tsx`, new assertion): the sentence reads `finishCarriedBefore` + "Settings" + "." and links to `/settings`.

- [ ] `fix(web): send each link into Settings to its section`

### Task 4: the spec and the design say so

Files: modified `docs/SPEC.md` (0.85 header sentence; section 8's web UI paragraph; section 4's Change project), `docs/design/web-ui.md` (Settings row, line 130), `docs/design/settings-directory.md` (Status: built on this branch)

Tests: none (docs).

- [ ] `docs(spec): describe Settings as a directory`

## Verification

```
pnpm check
# expected: exit 0, every package's tests passed
cargo xtask check --integration
# expected: xtask check: ok (the e2e journeys and narrow() checks included)
```

Then the founder opens Settings at desktop and phone width, walks every section, and follows Today's "Open Settings" link.
