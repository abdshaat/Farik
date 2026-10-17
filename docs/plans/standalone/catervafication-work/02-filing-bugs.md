# Standalone catervafication-work, plan 2: a hunter files bugs

Status: draft
Branch: `feat/catervafication-work` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 5.1, 5.16, 6.4, 6.8, 8.5, F5
Depends on: plan 1 of this branch
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

A task whose `allowed_paths` are exactly its role's hunt report is a hunt. No agent reviews it, so
the governor accepts it on its criteria (ADR 0063 amends ADR 0061's rule). In a hunt's implement
session its assignee can file each bug it finds with `catervas_file_bug`:
- the bug becomes a `draft` request created by the hunter, with its evidence in fixed words;
- `task.created` names the hunt in `found_by`;
- Catervas triages it small in the same filing, and the Product Manager refines it like any
  request;
- a hunt files at most 10 bugs, and never a second open bug of the same title.

Out of scope here:
- filing the hunts themselves (plan 3);
- any other web change: the bug is a request on the board like any other. The task page only
  says "Catervas checks it" for a hunt, as for any task no agent reviews (Task 4).

## Decisions

- `hunt_report(role, format)`:
  - `SoftwareDeveloper` → `docs/catervas/engineering/bug-hunt.<x>`;
  - `UiUxDesigner` → `docs/catervas/design/ui-bug-hunt.<x>`;
  - any other role → `None`.

  `<x>` is `format.extension()`, as `with_format` builds a name.
- `is_hunt(contract)` holds when all of these do:
  - `contract.kind == Task`;
  - `allowed_paths` is exactly one path, equal to `hunt_report(assignee_role, Markdown)` or
    `hunt_report(assignee_role, Html)`.

  A glob never matches. It is like a plan task, whose paths are exactly its plan (5.16). A
  parent is not required, so a hunt the owner gives in a chat (plan 4) is one too.
- `reviewed_by_an_agent(contract)` (ADR 0061, `crates/core/src/team.rs`) becomes `kind == Task &&
  changes_code(assignee_role) && !is_hunt(contract)`. Nothing else changes: plan 1 of ADR 0061
  already routes every task it is false for to the governor.
- `task.created` gains `found_by`, a task id (`^CTV-[0-9]{1,6}$`), optional. It is written only
  by `file_bug` and read by `bugs_found`.
- `file_bug` files a request as `file_request` does, with `Filing.found_by`, and in the same
  filing records `request.triaged { size: small, reason: "a bug <hunt> found", triaged_by:
  "catervas" }`, as `file_raise_request` triages a raise. So no triage session runs, and rule 10
  moves the bug to `refining` for the Product Manager.
- The bug's contract is `request_from_brief(title, intent, placeholder_budget_usd(rules))`. Its
  `assignee_role` and every other placeholder are the Product Manager's to rewrite, as for any
  request. The tool sets no `risk` from the severity: risk is the contract's.
- `catervas_file_bug` is offered only to the implement session of a task that `is_hunt`
  (`offered_tools`, `crates/runtime/src/orchestrator/session.rs`, beside the marketing tools'
  filter), so no other session is shown it. The refusals below still hold for a call.
- The task page's `reviewedByAnAgent` (`apps/web/src/pages/TaskDetail.tsx`, ADR 0061 plan 1) is
  false for a hunt too: a contract whose `allowedPaths` is exactly one path, equal to
  `docs/catervas/engineering/bug-hunt.md` or `.html` for `software_developer`, or
  `docs/catervas/design/ui-bug-hunt.md` or `.html` for `ui_ux_designer` (`isHunt`).
- `catervas_file_bug`, tier `Read`, input `FileBugInput` with `deny_unknown_fields`. Each length
  is in code points of the trimmed text:

  | field | limit |
  |---|---|
  | `title` | 3 to 120 |
  | `where` (Rust field `place`, `#[serde(rename = "where")]`) | 1 to 300 |
  | `steps` | 1 to 10 items, each 1 to 300 |
  | `expected` | 1 to 1,000 |
  | `actual` | 1 to 1,000 |
  | `severity` | `low`, `medium` or `high` |
  | `evidence` | 1 to 5 items, each 1 to 1,000 |
- The intent, in exact words, each part trimmed and its line breaks kept:

  ```
  Fix this bug, found by <hunter's display name> while hunting in <hunt id>: <actual>

  Where: <where>
  Steps:
  1. <step 1>
  2. <step 2>
  Expected: <expected>
  Actual: <actual>
  Severity: <severity>
  Evidence:
  - <evidence 1>
  ```
- Refusals, `ToolError::Refused`, checked in this order, nothing recorded on any of them:
  - `bug_refused: only a hunt's assignee files a bug, in the hunt's implement session`, when any
    of these fails:
    - `call.context.purpose == Implement`;
    - the session has a task, and its contract `is_hunt`;
    - the board row's `assignee_id` is the caller.
  - `bug_out_of_bounds: <field> <what is wrong>`, the first field out of its limit, in the input's
    order.
  - `hunt_bug_limit: <hunt> has filed 10 bugs, the most a hunt files; write your report and
    finish`, when `bugs_found` already lists `MAX_BUGS_PER_HUNT` bugs of this hunt, whatever
    their status.
  - `bug_already_filed: <id> is open with this title`. A title matches when, trimmed,
    lower-cased and with each run of whitespace collapsed to one space, it equals the title of a
    task that any hunt filed and whose board row is neither `accepted` nor `cancelled`.
- The answer is `{ "task_id": "<id>", "status": "draft", "filed": <bugs of this hunt now>,
  "left": 10 - filed }`.
- The hunt's report lists the bugs, so no event is added for "bug found": `task.created` with
  `found_by` is the record.

## File map

```
crates/core/src/folders.rs                    modifies: hunt_report, is_hunt, mod tests (Task 1)
crates/core/src/team.rs                       modifies: reviewed_by_an_agent (Task 1)
docs/schemas/event.schema.json                modifies: taskCreatedBody.found_by (Task 2)
crates/protocol/src/event.rs                  modifies: as generated (Task 2)
crates/store/src/requests.rs                  modifies: Filing.found_by, file_bug, bugs_found, mod tests (Task 2)
crates/runtime/src/tools/bugs.rs              creates:  FileBugInput, BugSeverity, MAX_BUGS_PER_HUNT, file_bug, bug_intent, mod tests (Task 3)
crates/runtime/src/tools.rs                   modifies: `mod bugs`, TOOLS entry, dispatch arm, its tool list test (Task 3)
crates/runtime/src/orchestrator/session.rs    modifies: offered_tools's filter, mod tests (Task 3)
apps/web/src/pages/TaskDetail.tsx             modifies: isHunt, reviewedByAnAgent (Task 4)
apps/web/src/pages/TaskDetail.test.tsx        tests:    a hunt's lead (Task 4)
```

## Interfaces

Consumes (on the branch): `TaskContract`, `TaskKind`, `Role`, `DocumentFormat`, `with_format`,
`role_folder`, `changes_code` (`catervas-core`); `reviewed_by_an_agent` (ADR 0061);
`file_request`'s `file` and `Filing`, `request_from_brief`, `placeholder_budget_usd`,
`RequestTriagedBody` (`catervas-store`); `Call`, `call.row`, `call.deps()`, `call.ids`,
`call.team`, `TOOLS` (`catervas-runtime` tools); `TestProject`, `filed_with`, `moved`, `call_in`,
`a_team_of_three` (`tools::fixtures`); `session_spec`, `asked`, `agent`, `offered_tools`
(`orchestrator::session` and its `mod tests`); `a_contract` (`governor::readiness::fixtures`).

Produces:

```rust
// catervas-core::folders
#[must_use] pub fn hunt_report(role: Role, format: DocumentFormat) -> Option<String>;
#[must_use] pub fn is_hunt(contract: &TaskContract) -> bool;

// catervas-store::requests
/// Files a bug a hunt found as a draft request created by `created_by`, `found_by` the hunt,
/// triaged small by Catervas in the same filing.
pub fn file_bug(files: &ProjectFiles, log: &EventLog, wire: Value, created_by: &str,
    found_by: &TaskId, now: DateTime<Utc>, ids: &EventIds) -> Result<TaskContract, RequestError>;
/// Every (bug, hunt) a `task.created` with `found_by` records, oldest first.
pub fn bugs_found(log: &EventLog) -> Result<Vec<(TaskId, TaskId)>, StoreError>;

// catervas-runtime::tools::bugs
pub const MAX_BUGS_PER_HUNT: usize = 10;
#[derive(Debug, Deserialize, JsonSchema)] #[serde(rename_all = "snake_case")]
pub(crate) enum BugSeverity { Low, Medium, High }
#[derive(Debug, Deserialize, JsonSchema)] #[serde(deny_unknown_fields)]
pub(crate) struct FileBugInput { title: String, #[serde(rename = "where")] place: String,
    steps: Vec<String>, expected: String, actual: String, severity: BugSeverity, evidence: Vec<String> }
pub(super) fn file_bug(call: &Call<'_>, input: &FileBugInput) -> Result<Value, ToolError>;
fn bug_intent(input: &FileBugInput, hunter: &str, hunt: &TaskId) -> String;
```

```ts
// apps/web/src/pages/TaskDetail.tsx
export function isHunt(contract: Contract): boolean;
```

## Tasks

### Task 1: a hunt, and that no agent reviews it

Files: `crates/core/src/folders.rs` (the two functions; `mod tests`), `crates/core/src/team.rs`
(`reviewed_by_an_agent`). Produces `hunt_report`, `is_hunt`.

- `knows_a_hunt_by_its_report_alone` (`folders.rs` `mod tests`):
  - Report paths:
    - `hunt_report(SoftwareDeveloper, Html)` is `docs/catervas/engineering/bug-hunt.html`;
    - `hunt_report(UiUxDesigner, Markdown)` is `docs/catervas/design/ui-bug-hunt.md`;
    - `hunt_report(Architect, Html)` is `None`.
  - With `a_contract()` as a `task` of `software_developer`:
    - `allowed_paths` `["docs/catervas/engineering/bug-hunt.html"]` is a hunt, and
      `team::reviewed_by_an_agent` is false for it;
    - with `src/**` added, it is not a hunt, and `reviewed_by_an_agent` is true;
    - `["docs/catervas/engineering/**"]` is not a hunt;
    - the first contract as an `epic` is not a hunt;
    - a `ui_ux_designer` task with the engineering report is not a hunt.

- [x] `feat(core): know a bug hunt by its report, which no agent reviews`

### Task 2: a bug is a request linked to its hunt

Files: `docs/schemas/event.schema.json` (`found_by` in `taskCreatedBody`, its description "The
hunt that found this bug (ADR 0063); written by catervas_file_bug alone."),
`crates/protocol/src/event.rs`, `crates/store/src/requests.rs`. The tests are in
`requests.rs`'s `mod tests`, which file into a temporary project as the other filing tests do.

- `files_a_bug_triaged_small_and_linked_to_its_hunt`, with a request filed first by
  `file_request` as `CTV-1`, standing in for the hunt:
  - `file_bug` with `request_from_brief("The save button does nothing", <an intent of 40
    characters>, 20.0)`, created by `dev-a`, found by `CTV-1`, files `CTV-2` as a `draft`
    `task`;
  - its `task.created` has `created_by` `dev-a` and `found_by` `CTV-1`;
  - the next event is `request.triaged { size: small, reason: "a bug CTV-1 found", triaged_by:
    "catervas" }` about `CTV-2`;
  - `bugs_found` answers `[(CTV-2, CTV-1)]`;
  - `CTV-1`, filed with `file_request`, is not in `bugs_found`.

- [x] `feat(store): file a bug as a request its hunt is named on`

### Task 3: catervas_file_bug

Files: created `crates/runtime/src/tools/bugs.rs`; modified `crates/runtime/src/tools.rs`
(`mod bugs;`, the `TOOLS` entry after `catervas_request_connection` with the description "In a
bug hunt: file one bug you found, with where it is, the steps, what you expected, what happened,
its severity and the evidence. It goes to the Product Manager as a request. At most 10 per hunt.",
the dispatch arm; `lists_every_tool_with_its_tier` lists it right after
`catervas_request_connection`, and its count of leading `Read` tools grows by one), and
`crates/runtime/src/orchestrator/session.rs` (`offered_tools`: `catervas_file_bug` only when
`ask.purpose == Implement` and `ask.contract.is_some_and(is_hunt)`). Consumes Tasks 1 and 2.

The fixture is a `TestProject` of `a_team_of_three(|_| {})`. `CTV-1` is filed with `filed_with(
"CTV-1", "assigned", "task", None, …)`, its contract's `assignee_role` `software_developer`
and `allowed_paths` `["docs/catervas/engineering/bug-hunt.html"]`. Its row names `dev-a` as
assignee by `moved("CTV-1", "assigned", "in_progress", &json!({ "assignee": "dev-a", "reviewer":
"pm" }))`, as the tests of `tools/work.rs` give a task its assignee.

- `files_a_bug_with_its_evidence`. `dev-a`, in an implement session on `CTV-1`, files `{ title:
  "The save button does nothing", where: "Settings, the Profile card", steps: ["Open Settings",
  "Press Save"], expected: "The name is saved", actual: "Nothing happens and the console logs
  TypeError", severity: "high", evidence: ["console: TypeError: x is undefined at save.ts:12"] }`:
  - the answer is `{ task_id: "CTV-2", status: "draft", filed: 1, left: 9 }`;
  - `CTV-2`'s contract has that title, and an intent equal, character for character, to the
    Decisions' format filled with those parts, the hunter's display name and `CTV-1`;
  - its `task.created` names `found_by` `CTV-1` and `created_by` `dev-a`.
- `holds_a_hunt_to_its_bugs` (the bound, the duplicate rule and the gate, one guarantee). It
  first files the bug of `files_a_bug_with_its_evidence`, as `CTV-2`. In this order:
  - the same title as `"  the SAVE button   does nothing "` is refused with `bug_already_filed:
    CTV-2 is open with this title`;
  - once `CTV-2` is `cancelled`, that title is filed;
  - after 10 bugs of `CTV-1`, an 11th is refused with `hunt_bug_limit: CTV-1 has filed 10
    bugs, `;
  - each of these is refused with `bug_refused: `:
    - `dev-b` on `CTV-1`;
    - `dev-a` from a `refine` session (`call_in`);
    - `dev-a` on a `CTV-20` whose `allowed_paths` are `["src/**"]`, filed and moved to
      `in_progress` with `dev-a` as assignee only now, after the bound, since a new id is taken
      past the highest contract;
  - `steps: []` is refused with `bug_out_of_bounds: steps`;
  - no `task.created` follows any refusal.
- `offers_file_bug_to_a_hunts_implement_session_alone` (`session.rs` `mod tests`, ignored, as
  `offers_the_seller_tools_to_procurement_alone` is, and built the same way with
  `session_spec(..).catervas_tools`): `dev-a`'s implement session of a hunt contract (the one
  above) is given `catervas_file_bug`; its refine session of that contract, and its implement
  session of a contract whose `allowed_paths` are `["src/**"]`, are not.

- [x] `feat(runtime): a hunt files the bugs it finds, ten at most and none twice`

### Task 4: the task page says Catervas checks a hunt

Files: `apps/web/src/pages/TaskDetail.tsx` (`isHunt`, and `reviewedByAnAgent` false when it
holds), tested by `apps/web/src/pages/TaskDetail.test.tsx`. Consumes ADR 0061 plan 1's
`reviewedByAnAgent` and its strings `taskChecked` and `gateChecked`.

- `says_catervas_checks_a_hunt`: a Developer's task whose `allowedPaths` are
  `["docs/catervas/engineering/bug-hunt.html"]` has a lead holding "Catervas checks it" and no
  "reviews it"; with `["docs/catervas/engineering/bug-hunt.html", "src/**"]` the lead holds "Ada
  reviews it", as ADR 0061's test sets the reviewer.

- [x] `feat(web): say Catervas checks a bug hunt`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok   (Task 3's ignored session test among them)
```
