# Standalone internal-task-ids, plan 1: the names

Status: draft
Branch: `feat/internal-task-ids` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 5.11, 5.14 (recorded by plan 6)
Depends on: `feat/parallel-agents` landed beneath this branch (README)
Readiness confirmed by: the planner, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

`catervas-core` can name a task's change and its branch with no id in them, and can say what goes
to the forge with no id in it. The contract can record the branch Catervas chose, and
`task_branch` answers that record, else the old rule. Nothing calls the new names yet (plans 2 to
5 do), so no behaviour a user sees changes here.

## Decisions

- The type, the slug, the suffix, the record and the pull request's words are ADR 0066's. They are
  not restated here; the tests below pin them.
- `branch` joins `FIELDS_THE_STORE_OWNS`, chosen over a new seventh set. The set already means
  "Catervas writes it and no actor writes it through a contract tool", which is exactly `branch`.
  A new set would need a new refusal for the same answer.
- The schema's pattern for `branch` is the new shape alone:
  `^(feat|fix|docs)/[a-z0-9]+(-[a-z0-9]+)*$`, maxLength 64. An old task never records one (plan 2).
  64 holds `docs/` (5), a 40-character slug, and a suffix up to `-999999`.
- `forge_text` is a new module, not more of `branch.rs`. A pull request's words and a commit
  message are about what leaves Catervas, not about naming a branch.
- `without_task_ids` and `task_id_in` read an id as `CTV-` followed by one or more ASCII digits.
  A word start means a preceding character that is not ASCII alphanumeric, or none.
  - `without_task_ids` replaces a match only when it starts a word, the character before it is not
    `/`, and its digits parse as a `TaskId`. A match of seven digits is left as it is.
  - `task_id_in` counts any match that starts a word, a `/` before it included. A path named in a
    commit message is still a task id in a commit.

## File map

```
docs/schemas/task-contract.schema.json   modifies: `branch` property (Task 1)
crates/core/src/branch.rs                modifies: ChangeType, change_type, SLUG_MAX, branch_slug, free_branch, old_task_branch; task_branch reads `branch` (Task 1)
crates/core/src/branch.rs                tests:    names_a_task_branch_from_its_type_and_title replaces the four naming tests (Task 1)
crates/core/src/governor/gates.rs        modifies: FIELDS_THE_STORE_OWNS gains "branch", its doc (Task 1)
crates/core/src/governor/gates.rs        tests:    keeps_the_lock_the_humans_and_the_identifiers_the_stores lists "branch" (Task 1)
crates/core/src/forge_text.rs            creates:  pull_request_title, without_task_ids, task_id_in (Task 2)
crates/core/src/forge_text.rs            tests:    forge_text_names_no_task (Task 2)
crates/core/src/lib.rs                   modifies: pub mod forge_text (Task 2)
```

`crates/store/src/requests.rs`'s `fields_not_the_authors` reads `FIELDS_THE_STORE_OWNS`, so a
filing that sets `branch` is refused with no change there.

## Interfaces

Consumes: `TaskContract`, `TaskId` (`catervas_core::contract`), `changes_code`
(`catervas_core::team`), `CatervasTaskContractChange` (generated).

Produces:

```rust
// crates/core/src/branch.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType { Feat, Fix, Docs }
impl ChangeType {
    /// `feat`, `fix`, `docs`: a branch's first segment.
    #[must_use] pub fn prefix(self) -> &'static str;
    /// `Feat`, `Fix`, `Docs`: a pull request title's first word.
    #[must_use] pub fn word(self) -> &'static str;
}
/// ADR 0066: Docs when the role changes no code or every allowed path starts with `docs/`;
/// else Fix when `change` is `fix`; else Feat.
#[must_use] pub fn change_type(contract: &TaskContract) -> ChangeType;
pub const SLUG_MAX: usize = 40;
/// ADR 0066's slug of `words`; `change` when nothing is left.
#[must_use] pub fn branch_slug(words: &str) -> String;
/// `<prefix>/<slug>`, else with `-2`, `-3`, ... appended, the first that `taken` does not hold.
/// A name is held when an entry equals it or starts with it and a `/`.
#[must_use] pub fn free_branch(kind: ChangeType, slug: &str, taken: &[String]) -> String;
/// The rule before ADR 0066: `feature/<id>`, `fix/<id>`, `docs/<id>`.
#[must_use] pub fn old_task_branch(contract: &TaskContract) -> String;
/// The recorded `branch`, else `old_task_branch`.
#[must_use] pub fn task_branch(contract: &TaskContract) -> String;
// `task_number_of_branch` is unchanged.

// crates/core/src/forge_text.rs
/// `<word>: <title>`, the title passed through `without_task_ids`.
#[must_use] pub fn pull_request_title(
    kind: ChangeType, title: &str, title_of: &dyn Fn(&TaskId) -> Option<String>,
) -> String;
/// `text` with each id, as the Decisions define it, replaced by `"<its title>"`, or by
/// `another task` when `title_of` answers `None`.
#[must_use] pub fn without_task_ids(text: &str, title_of: &dyn Fn(&TaskId) -> Option<String>) -> String;
/// The first task id `text` names (`CTV-12`), as the Decisions define it.
#[must_use] pub fn task_id_in(text: &str) -> Option<String>;
```

`TaskContract` gains `branch: Option<…>`, the type typify generates from the schema's constrained
string (the way `repo` and `plan` are generated). It is read with `.as_str()` and built with
`.parse()` or `TryFrom<&str>`, as those are.

## Tasks

### Task 1: a task's branch is named from its type and title, and recorded

Files: modified `docs/schemas/task-contract.schema.json`, `crates/core/src/branch.rs`,
`crates/core/src/governor/gates.rs`.
Produces: `ChangeType`, `change_type`, `SLUG_MAX`, `branch_slug`, `free_branch`,
`old_task_branch`, `task_branch`, the contract's `branch`.
Consumes: nothing new.

The schema's `branch` property:
- description: "The branch the task works on, chosen by Catervas from its type and title when its
  worktree is first made, and never changed (ADR 0066). Absent until then, and on a task started
  before 0.119, whose branch is feature/, fix/ or docs/ and its id. Written only by Catervas."
- the pattern and maxLength above.

`FIELDS_THE_STORE_OWNS` becomes `[&str; 5]`, with `"branch"` last. Its doc says "...and the branch
Catervas records when the task's worktree is first made". Every fixed-size use of the constant is
updated where the compiler points.

The module doc of `branch.rs` is rewritten:
- what ADR 0066 decides;
- `task_branch` is where every caller reads a task's branch;
- `old_task_branch` and `task_number_of_branch` are kept for tasks started before 0.119.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `names_a_task_branch_from_its_type_and_title` replaces `names_a_developers_feature_branch`,
  `names_a_developers_fix_branch`, `puts_a_designers_task_on_a_feature_or_fix_branch` and
  `names_every_other_roles_branch_docs`. It asserts, from `a_contract()` with its fields set:
  - `change_type`:
    - a Software Developer's task with no `change` is `Feat`, and with `change: fix` is `Fix`;
    - a UI/UX Designer's with `fix` is `Fix`;
    - an Architect's with `fix` is `Docs`;
    - a Software Developer's whose `allowed_paths` are `["docs/catervas/engineering/bug-hunt.md"]`
      with `fix` is `Docs`;
    - a Software Developer's whose paths are `["docs/api.md", "src/**"]` is `Feat`.
  - `prefix()` and `word()` are `feat`/`Feat`, `fix`/`Fix`, `docs`/`Docs`.
  - `branch_slug`:
    - `"Character counter on the note input"` is `character-counter-on-the-note-input`;
    - `"  Fix: the café's £5 bug!! "` is `fix-the-caf-s-5-bug`;
    - `"???"` is `change`;
    - `"Make the onboarding checklist remember skipped steps across devices"` is
      `make-the-onboarding-checklist-remember` (cut at the last `-` within 41 characters);
    - 45 `a`s are 40 `a`s.
  - `free_branch(Feat, "add-a-login-page", taken)`:
    - is `feat/add-a-login-page` with `taken` empty;
    - is `feat/add-a-login-page-2` with `taken` holding `feat/add-a-login-page`;
    - is `feat/add-a-login-page-3` with it holding that and `feat/add-a-login-page-2/x`;
    - is `feat/add-a-login-page` with it holding `feat` (git refuses it later, as ADR 0066 says).
  - `task_branch`:
    - is `feature/CTV-7` for a Software Developer's contract with no `branch`;
    - is `docs/CTV-7` for an Architect's;
    - is `feat/add-a-login-page` once `branch` is set to it.
- `task_number_of_branch_reads_the_three_shapes` and `task_number_of_branch_inverts_task_branch`
  stay. The second calls `old_task_branch`.
- `keeps_the_lock_the_humans_and_the_identifiers_the_stores`: its loop over the store's fields
  lists `"branch"` too, refused `StoresFields`.
- `gives_every_field_of_the_schema_to_exactly_one_owner` passes unchanged. It fails until
  `"branch"` is in a set, which is the reason to watch.

- [x] `feat(core): a task's branch is named from its type and title, and recorded`

### Task 2: what leaves for the forge names no task

Files: created `crates/core/src/forge_text.rs`, modified `crates/core/src/lib.rs`.
Produces: `pull_request_title`, `without_task_ids`, `task_id_in`.
Consumes: `ChangeType` (Task 1).

The module doc says what reaches the forge (a pull request's title and body, a commit's message)
and cites ADR 0066.

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `forge_text_names_no_task` asserts, with `title_of` knowing `CTV-2` as "Pie pre-orders" and
  nothing else:
  - `pull_request_title(Feat, "Character counter on the note input", ..)` is
    `Feat: Character counter on the note input`;
  - `pull_request_title(Fix, "Undo CTV-2's rounding", ..)` is
    `Fix: Undo "Pie pre-orders"'s rounding`.
  - `without_task_ids`:
    - `"Follows CTV-2; see docs/catervas/architecture/plans/CTV-2.md. CTV-9 is next. (CTV-2)"` is
      `Follows "Pie pre-orders"; see docs/catervas/architecture/plans/CTV-2.md. another task is next. ("Pie pre-orders")`;
    - `"ACTV-2 and CTV-1234567"` is unchanged;
    - a text with no id is unchanged.
  - `task_id_in`:
    - `"feat: add the counter (CTV-1)"` is `Some("CTV-1")`;
    - `"docs: write plans/CTV-12.md"` is `Some("CTV-12")`;
    - `"feat: CTV-x and ACTV-3"` is `None`;
    - `"fix: round the total"` is `None`.

- [ ] `feat(core): what leaves for the forge names no task`

## Verification

```
cargo xtask check
# expected: xtask check: ok
```
