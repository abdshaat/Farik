# Standalone (open issues), plan 7: the web app's fixes and its stale words

Status: draft
Branch: `fix/open-issues` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.1, 5.12, 7 F8 (recorded in plan 8's revision 0.122)
Depends on: plan 6 of this folder (committed on this branch)
Readiness confirmed by: none (ADR 0065); the author held it to the three rules

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

Setup's spending step shows its example as an example, not as the team's spend ($128.00 before any
project existed); Settings → How work runs sets the integration branch; `/chats` opens the chats;
and the words that still describe approvals, a reviewer of every piece of work, or chats that only
suggest work say what the product does since ADRs 0061 and 0063. Out of scope: any visual change
(the redesign is on its own branch), and the strings of pages that only an older log reaches (plan
approval, the acceptance gate, the Designer's plan returns), which stay for those logs.

## Decisions

Every decision is ADR 0069's (decision 8); this section pins how.

- **The example** (`pages/setup/SetupSpending.tsx`, `useDailyLimit`): the line becomes the new key
  `spendExampleLine: "Say the team spends {spent} in a day: {left} of {limit} is left."` with `spent`
  `money(dollars * EXAMPLE_SPENT)` and `left` `money(dollars - dollars * EXAMPLE_SPENT)`.
  `costsTodayOf` stays the Costs page's alone. The meter and `spendExampleStop` are unchanged; the
  hook's two other users (`dialogs/DailyLimit.tsx`, `TeamRules.tsx`) get the same line.
- **The integration branch** (`policy.integration_branch`, `team.schema.json` already allows 1 to
  200 characters). Backend, in `checked` (`daemon/team.rs`), when the wire team sets it: a name
  `deps.git.check_branch_name` refuses is the error `integration_branch_name` at
  `/policy/integration_branch`; a name with no local branch (`deps.git.has_local_branch`) is
  `integration_branch_missing` there; `code_of` maps both, as it maps its other codes. A project of
  several repositories (ADR 0057) keeps each member's branch in `repos.yaml`: `team.get` gains
  `members_present: bool` (`teamGetResult` in `docs/schemas/rpc.schema.json`, required), true when
  the project folder's `repos.yaml` lists members (read as `daemon/repos.rs` reads it); there the
  field is not shown and `checked` makes neither check.
  `describe_change` (`crates/core/src/team/describe.rs`) says "Finished work goes into {branch}." when
  the value changes to a name, and "Finished work goes into the repository's main branch." when it is
  removed. Web: `IntegrationBranch` in `pages/TeamRules.tsx`, a `TextField` as `UiPaths` has, saved
  through `useChange` as `Documents` saves `documentFormat`: `{...saved, policy: {...saved.policy,
  integrationBranch}}`, the key dropped when the field is empty; `Work.tsx` shows it after
  `Finish`. The Team type (`TeamSetup.tsx`) gains `integrationBranch?: string`. Words:
  `rulesBranch: "Where finished work goes"`, `rulesBranchHint: "The branch pull requests go into.
  Leave it empty for the repository's main branch."`, `rulesBranchLabel: "Branch"`,
  `refuseBranchName: "Git does not allow that as a branch name."`, `refuseBranchMissing: "This
  project has no branch of that name yet. Make it, then save."`, and `sectionWorkSummary` becomes
  "How documents are written, and where finished work goes". `refusals.ts`'s `WORDS` maps the two
  codes.
- **`/chats`** (`app/App.tsx`): `<Route path="/chats" element={<Navigate to="/channel" replace
  />} />` and `<Route path="/chats/:id" element={<ChatsAlias />} />`, where `ChatsAlias`
  (`pages/ChatsAlias.tsx`) navigates to `/channel/${id}` with `replace`. `/channel` stays the place
  the menu and every link use, so no pinned path changes.
- **The words** (`strings/en.ts`), each key's new text:
  - `jobDesigner`: "Looks at your app the way a customer does, plans changes to its screens and makes them. Checks every screen {developer} builds." (`SetupTeam.tsx` stops passing `pm`.)
  - `jobArchitect`: "Keeps the project well built, and reviews every change to its code."
  - `checkReviewed`: "An agent reviews every change to code"; `checkReviewedNote`: "Other work is checked by the checks Catervas runs."
  - `ruleDeveloper`: "Only the Developer and the UI/UX Designer change code"; `ruleDeveloperNote`: "Everyone else changes documents and plans."
  - `checkNewHint`: "Say what must be true of finished work, in at least 10 characters."
  - `chatsIntro`: "{name} answers from memory and from reading the project. Work you ask {name} for becomes {name}'s task; work for someone else comes back as a request for you to send."
  - `chatsIntroPhone`: "{name} answers, and takes on the work you ask for."
  - `questionsLead`: "{agent} asked these about {id}. Your answers go into the work."
  - `pausedStillYours`: "You can still answer questions and merge pull requests."
  - `toolApprovalStopped`: "{agent} stopped to ask before something that costs money."
  - `finishTitle`: "When a piece of work is done, what happens to it?"
  - `metricNeededNote`: "Answers, payments, and help when stuck."; `metricChecksNote`: "The others are judged by a reviewer."
  - `refusePlanNoReviewer`: "To start the product plan, your team needs an Architect or a Scrum Master. Add one on the Team page."
  - `refuseCatervaficationNoWriters`: "To start, your team needs an Architect or a Scrum Master. Add one on the Team page, then start."
  - `nextSmall3` and `nextSmall4` are deleted (nothing uses them).
- **The guard**: a test lists the phrases that must not come back (below).

## File map

```
apps/web/src/pages/setup/SetupSpending.tsx      modifies: the example line (Task 23)
apps/web/src/pages/setup/team.test.tsx          tests:    shows_an_example_of_the_limit amended (Task 23)
crates/runtime/src/daemon/team.rs               modifies: checked's two refusals, code_of, team.get's members_present (Task 24)
docs/schemas/rpc.schema.json                    modifies: teamGetResult.members_present (Task 24)
crates/runtime/src/daemon/team.rs               tests:    refuses_an_integration_branch_the_project_does_not_have (Task 24)
crates/core/src/team/describe.rs                modifies: the effect line (Task 24)
apps/web/src/pages/TeamRules.tsx, pages/settings/Work.tsx, pages/setup/TeamSetup.tsx, app/refusals.ts  modifies (Task 24)
apps/web/src/pages/settings/settings.test.tsx   tests:    sets_the_integration_branch; line 224 amended (Task 24)
apps/web/src/app/App.tsx, pages/ChatsAlias.tsx  modifies, creates (Task 25)
apps/web/src/pages/Chats.test.tsx               tests:    opens_the_chats_at_chats (Task 25)
apps/web/src/strings/en.ts                      modifies: Tasks 23, 24 (new keys), 26 (the words)
apps/web/src/strings/stale-words.test.ts        creates:  says_nothing_the_product_no_longer_does (Task 26)
```

## Interfaces

Consumes (on this branch): `useDailyLimit`, `EXAMPLE_SPENT`, `money` (`SetupSpending.tsx`);
`checked`, `code_of`, `errors_wire` (`daemon/team.rs`); `Git::check_branch_name`,
`Git::has_local_branch` (store); `describe_change`; `useRuleParts`, `useChange`, `UiPaths`,
`Documents`, `PutBack` (`TeamRules.tsx`); `TextField` (`@catervas/ui`); `WORDS` (`refusals.ts`);
`Chats` and its route; `en`.

Produces: the keys above, `IntegrationBranch` (a component in `TeamRules.tsx`), `ChatsAlias`.

## Tasks

### Task 23: the spending example reads as one

Files: modified `SetupSpending.tsx`, `en.ts`; tested by `pages/setup/team.test.tsx`

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `shows_an_example_of_the_limit` (amended) — at a $20.00 limit the step reads "Say the team spends
  $12.80 in a day: $7.20 of $20.00 is left." and not "Today the team has spent". Fails first on the
  old line.

- [x] `fix(web): setup's spending example no longer reads as the team's spend`

### Task 24: Settings sets the integration branch

Files: as the file map says for Task 24

Tests:

- `refuses_an_integration_branch_the_project_does_not_have` (`daemon/team.rs`, ignored "needs the git
  program") — `team.validate` with `policy.integration_branch` `"catervas/next"` answers
  `integration_branch_missing` until a local branch `catervas/next` exists, then no error and the
  effect "Finished work goes into catervas/next."; `"bad..name"` answers `integration_branch_name`;
  `team.get` answers `members_present: false`.
- `sets_the_integration_branch` (`settings.test.tsx`) — on `/settings/work`, typing `catervas/next`
  and saving calls `team.save` with `policy.integrationBranch` `"catervas/next"`; clearing it and
  saving calls `team.save` with no `integrationBranch`; the section lists `en.rulesFinish`,
  `en.rulesBranch`, `en.rulesDocuments` (line 224's expectation); with `membersPresent: true` the
  field is absent. Fails first: no field.

- [x] `feat(web): settings sets the branch finished work goes into`

### Task 25: `/chats` opens the chats

Files: modified `App.tsx`; created `pages/ChatsAlias.tsx`; tested by `Chats.test.tsx`

Tests:

- `opens_the_chats_at_chats` — rendering the app's routes at `/chats` shows the chats page (as
  `/channel` does), and at `/chats/theo` Theo's one-to-one chat; neither shows `en.noPage`.

- [x] `fix(web): /chats opens the chats`

### Task 26: the stale words

Files: modified `en.ts`, `SetupTeam.tsx` (no `pm` for `jobDesigner`), and each test the compiler
or `pnpm check` names that pins an old literal (`team.test.tsx:226` and `:331`,
`Chats.test.tsx:1143` and `:1162`); created `strings/stale-words.test.ts`

Tests:

- `says_nothing_the_product_no_longer_does` — no value of `en` contains "once {pm} agrees",
  "suggests a request, and you decide", "asks before writing the plan", "Someone on the team
  reviewed it", "approve and accept", "reviews the work before you see it", "Another agent checks
  it", "checks the plan before you read it" or "have their notes checked"; and `en` has no key
  `nextSmall3` or `nextSmall4`. Fails first on "once {pm} agrees".

- [x] `fix(web): words that match the product since only code is reviewed and chats take work`

## Verification

```
cargo xtask check
# expected: xtask check: ok (pnpm check included)
cargo xtask check --integration
# expected: xtask check: ok, with the git test of daemon/team.rs run
```

In this cloud container, `/tmp/claude-0/fullcheck.sh` runs what `--integration` runs; say which ran.
