# Phase 2, step 01: Protocol crate

Status: ready
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; a session may not push to another branch without permission, so phase 2 reuses it as phase 1 did; steps do not get their own)
Spec: `docs/SPEC.md` section 8.5 (event protocol), 8.4 (storage), 5.11 (contract ownership), 5.13 (the criterion library), 5.16 (triage); `docs/standards/code.md`, "Wire and file formats" and "Schema validation"
Depends on: phase 0 (merged in #4), phase 1 (merged in #5)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a fresh Claude Code review session, 2026-09-17, on the fourth round. It rebuilt the step outside the working tree from the plan's own fenced blocks, applied verbatim in task order, and reproduced every expected output: each RED's compiler errors, each GREEN's test counts, `cargo fmt --all --check` clean after every task, all ten commit subjects accepted by `cargo xtask commit-msg`, the File map exact, and `cargo xtask check` ending in `xtask check: ok`. The three earlier rounds are recorded in the commits that took their findings.

## Goal

Farik's log has a shape. When this step is done, `docs/schemas/event.schema.json` says exactly what one record of the append-only log looks like, `docs/schemas/command.schema.json` says exactly what a request to change something looks like, and `farik-protocol` carries both as Rust types generated from those schemas, with one reader that turns an untrusted value into a typed event and one writer that turns a typed event back. It also carries the two traits, `Clock` and `IdSource`, that keep the machine's clock and its randomness out of every crate that decides anything. Nothing appends, reads, or displays an event yet; step 02 opens the log and step 03 projects it, and both of them consume what this step produces rather than inventing their own shapes.

## Decisions

- `farik-protocol` depends on `farik-core` and on nothing else of Farik's: `docs/plans/project-plan.md`, phase 2 decisions.
- Schemas own their types; `cargo xtask generate` writes the Rust module and a verbatim copy of the schema into `crates/<owner>/src/generated/`, and `cargo xtask check` fails when either is stale: `docs/plans/project-plan.md`, every-phase decisions; ADR 0005.
- The wire form of an event is a flat envelope with `kind` and `body`: chosen over a `oneOf` of nine whole events because the generator turns that `oneOf` into an untagged enum with positional variants whose `kind` is an unconstrained `serde_json::Value`, which is neither a domain type nor a check. Verified against `typify` 0.8.0 before this plan was written.
- The pairing of `kind` and `body` is checked by the reader, not by the schema: JSON Schema can say it with `allOf` of `if` and `then`, and `typify` 0.8.0 panics on that schema rather than generating from it. The reader dispatches on `kind` and refuses a body that does not fit it, with one test per kind, and the schema's description says so. Rejected: leaving the pairing unchecked, which would let a `contract.locked` event carry a `task.created` body into the log.
- Body shapes are distinct in their required properties, so `body` is a plain `oneOf` and a value matches exactly one branch. The reader never relies on that: it reads the body by `kind`, because the kind is what the event says it is, and shape is only what it happens to look like.
- The generator gains `PartialEq` on every generated type (`TypeSpaceSettings::with_derive`). Without it no event can be compared to an expected one in a test, and every assertion would have to go through the wire form. It regenerates `crates/core/src/generated/` in this step; the change is additive and `cargo xtask check` passes with it. Rejected: hand-writing the body structs to get the derive, which would abandon the schema as the source of types.
- The generated body structs are the crate's body types, re-exported rather than mirrored. Only the pieces the generator cannot express are hand-written: `EventEnvelope`, the tagged `EventBody`, `FarikEvent`, `NewEvent`, and `Command`. This is the "one hand-written type and one conversion at its edge" rule of the every-phase decisions.
- Strings in the event and command schemas carry no `minLength`. The generator turns a `minLength` into a newtype with `FromStr`, and a wrapper around a name that the log only prints buys nothing while making every construction fallible. The fields that must match a shape (`task_id`, `parent`) keep their `pattern`, where the wrapper is worth having, and the rules about blankness that matter — a blank `team_id` or `project_id` — are the reader's and `new_event`'s, with their own tests.
- `event_to_value` writes the wire form itself, field by field, rather than deriving `Serialize` on the domain types and calling `serde_json::to_value`. `serde_json::to_value` returns a `Result` that cannot be an error here, and `docs/standards/code.md` allows no `unwrap` or `expect` outside tests and `LazyLock` initialisers; a writer that returns a `Result` nobody can trigger would push that impossible error onto every caller forever. The round-trip test per kind is what keeps the hand-written writer honest against the generated reader.
- `ValidationError` is `farik_core::contract::ValidationError`, re-exported. `farik_core::pricing` already re-exports it for the price table; one shape of schema-validation error across the workspace is worth more than a name that matches its module.
- The contract vocabulary is not duplicated silently. `contract_summary` in the event schema repeats the `kind`, `status`, and `risk` value lists of `task-contract.schema.json`, because one schema never references another, and a test in `farik-protocol` compares the two lists and fails when they drift. `farik_core::contract::SCHEMA_JSON` becomes public so that the test can read the contract schema the crate embeds. Rejected: typing those three fields as plain strings, which would leave the front end generating `string` where it should generate a union.
- `contract_summary` carries only what phase 2 writes and the board shows: `kind`, `parent`, `title`, `status`, `risk`. `assignee`, `reviewer`, `sprint`, and `iteration` are not in it, because no event in this phase sets them; the step that first emits an event that does adds the field, per the every-phase decision that event kinds grow with the code.
- `Command::TaskCreate` boxes its contract. A `TaskContract` is an order of magnitude larger than the other command's arguments and an enum is as large as its largest variant, so `clippy::large_enum_variant`, which `cargo xtask check` runs with `-D warnings`, refuses the unboxed form. This makes the field `Box<TaskContract>` where `docs/plans/project-plan.md` writes `TaskCreate { contract: TaskContract }`; the project plan's signatures are abbreviated by its own statement, and this is the step plan making one exact.
- `EventError` carries no `Display` and no `std::error::Error`. `docs/standards/code.md` names `thiserror` for a crate's error enum, and the workspace pins no such dependency: `farik-core`'s seven error and refusal enums are plain enums the caller matches on, and one crate deviating would be the odd one out. The step that folds this into `StoreError` decides whether the workspace takes the dependency.
- All nine of this phase's event kinds land in this step, though this step emits none of them. The every-phase decision says the step that first emits a kind adds it to the schema; the same project plan's step 01 interface list names all nine here, because the schema is one file and the crate that owns it is built once. Steps 02, 03, 05, and 06 are what emit them.
- `pointer` and `read_body` are written once in `event.rs` and once in `command.rs` rather than shared. `farik-core` already keeps one `pointer` in `contract.rs` and an identical one in `pricing.rs`, so this is the house's answer for a seven-line helper at two schema boundaries, and a module holding two of them would be a namespace for functions, which `docs/standards/code.md` rules out. The two `read_body`s differ in their message anyway: one names an event kind, the other a command.
- The command schema gets a reader and no writer. Nothing in this phase puts a command on a wire: the command line builds a `Command` in process. A writer would have no caller and no test that means anything.
- `task_create`'s `contract` is typed `object` and nothing more. One schema never references another, and the contract's rules — the repeated criterion and requirement ids among them — live in `farik_core::contract::validate_contract`, which the reader calls, so a contract that arrives inside a command is held to exactly the rules a contract that arrives alone is.
- `Clock` and `IdSource` are traits with no supertraits, as `docs/plans/project-plan.md` states them. `FixedClock` and `SequentialIds` ship in the crate rather than behind `#[cfg(test)]`, so that every other crate's tests can use them, as `contract::fixtures` does.

## Design

`docs/schemas/event.schema.json` describes one record of the log: `seq`, `recorded_at`, `team_id`, `project_id`, and the optional `task_id`, `agent_id`, `session_id`, plus `kind` from a list of nine and `body` from a `oneOf` of nine shapes. `docs/schemas/command.schema.json` describes a command the same way, with two names and two bodies. `cargo xtask generate` turns each into a module under `crates/protocol/src/generated/` next to a copy of its schema.

On top of the generated types, `farik-protocol` hand-writes the four things the generator cannot: `EventEnvelope` (the envelope with `task_id` as `farik-core`'s `TaskId`), `EventBody` (a tagged enum over the generated body structs), `FarikEvent`, and `NewEvent` with `new_event`. `event_from_value` validates against the embedded schema, reads the envelope, then reads the body by kind and refuses one that does not fit. `event_to_value` writes the same shape back. `command.rs` does the reading half for commands, calling `validate_contract` for the contract inside `task_create`. `clock.rs` holds `Clock`, `IdSource`, and the two test doubles.

Out of scope for this step: the SQLite log and `EventQuery` (step 02), projections (step 03), any event kind this phase does not emit, the RPC types (phase 5 step 01), and a writer for commands.

## Architecture notes

Creates `crates/protocol` (`farik-protocol`), the third crate in the workspace. It consumes `farik_core::contract::{TaskContract, TaskId, ValidationError, validate_contract}` and `farik_core::contract::SCHEMA_JSON`, which this step makes public; everything else it needs it generates. It is consumed by nothing yet.

Modifies `xtask/src/generate.rs`, which is where `GENERATED_SCHEMAS` lives and where the generator's settings are, and therefore regenerates `crates/core/src/generated/`. Modifies `docs/SPEC.md` section 8.5, whose list of event kinds is the checklist phase 6 step 08 confirms against the code, and which does not yet name six of the nine kinds this step adds.

No crate gains a dependency the workspace does not already pin.

## Global constraints

- `farik-core` does no I/O; this step adds no code to it but one public constant over an `include_str!`, which is compile-time.
- `farik-protocol` reads no clock, no environment, and no file. Time and identifiers arrive through `Clock` and `IdSource`.
- Generated files are written only by `cargo xtask generate` and never edited.
- Event kinds are `<entity>.<past_tense_verb>`; wire and file keys are `snake_case`.
- Every public item carries a doc comment; a public function that can fail carries `# Errors`.
- Commits follow `docs/standards/code.md`; this plan's checkboxes are ticked in the same commits.

## File map

```
xtask/src/generate.rs                                  modifies: the PartialEq derive, and the two new schema entries
crates/core/src/generated/task_contract.rs             modifies (generated): gains PartialEq
crates/core/src/generated/prices.rs                    modifies (generated): gains PartialEq
crates/core/src/contract.rs                            modifies: SCHEMA_JSON becomes public
Cargo.toml                                             modifies: farik-core as a workspace path dependency
Cargo.lock                                             modifies (generated by cargo): the new member
docs/schemas/event.schema.json                         creates: one record of the log
docs/schemas/command.schema.json                       creates: one request to change something
crates/protocol/Cargo.toml                             creates: the crate manifest
crates/protocol/src/lib.rs                             creates: the module declarations
crates/protocol/src/generated/mod.rs                   creates: declares event and command
crates/protocol/src/generated/event.rs                 creates (generated): the event wire types
crates/protocol/src/generated/event.schema.json        creates (generated): the schema copy
crates/protocol/src/generated/command.rs               creates (generated): the command wire types
crates/protocol/src/generated/command.schema.json      creates (generated): the schema copy
crates/protocol/src/event.rs                           creates: the envelope, the bodies, the reader, the writer, new_event
crates/protocol/src/event/fixtures.rs                  creates: builders for test events
crates/protocol/src/command.rs                         creates: Command and its reader
crates/protocol/src/clock.rs                           creates: Clock, IdSource, FixedClock, SequentialIds
docs/SPEC.md                                           modifies: section 8.5 names every kind this phase adds
docs/plans/project-plan.md                             modifies: the status line, and the every-phase note on the generator's derives
CLAUDE.md                                              modifies: the current-state paragraph
README.md                                              modifies: the status line
docs/plans/phase-2-protocol-store-cli/step-01-protocol-crate.md   modifies: checkboxes ticked per task
```

## Tasks

### Task 1: Generated types compare by value

Files: modified `xtask/src/generate.rs`, `crates/core/src/generated/task_contract.rs`, `crates/core/src/generated/prices.rs`; tested by `xtask/src/generate.rs`

Consumes: `xtask::generate::{GENERATED_SCHEMAS, generate_types}` on `main`
Produces: every generated type derives `PartialEq`

- [x] Write the failing test. Insert it in the `tests` module of `xtask/src/generate.rs`, immediately above `generates_a_formatted_module_with_the_header_and_the_type`. The probe schema's one property is `required` on purpose: an object whose every property is optional also gets `Default` in its derive list, and the assertion would then never hold.

  ```rust
      #[test]
      fn derives_partial_eq_so_that_a_generated_value_can_be_compared_to_an_expected_one() {
          // Without it, a test that builds an event can only assert on its wire form, which is the
          // thing the writer is supposed to be checked against.
          let schema = r#"{"title": "Thing", "type": "object", "required": ["name"], "properties": {"name": {"type": "string"}}}"#;
          let module = generate_types(&GENERATED_SCHEMAS[0], schema).expect("generated");
          assert!(
              module.contains(
                  "#[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug, PartialEq)]"
              ),
              "{module}"
          );
      }
  ```

- [x] Run it and confirm it fails because the derive is missing:

  ```
  cargo test -p xtask generate::tests::derives_partial_eq
  # expected: FAIL, the assertion prints the module, whose derive reads
  #           #[derive(::serde::Deserialize, ::serde::Serialize, Clone, Debug)]
  ```

- [x] Write the minimal implementation. In `generate_types`, after `settings.with_struct_builder(false);`, add one line:

  ```rust
  settings.with_derive("PartialEq".to_string());
  ```

- [x] Run the test and the crate's suite; confirm green:

  ```
  cargo test -p xtask
  # expected: all passing
  ```

- [x] Regenerate the committed files and confirm the workspace still builds:

  ```
  cargo xtask generate
  # expected: generated crates/core/src/generated/task_contract.rs
  #           generated crates/core/src/generated/prices.rs
  cargo xtask check
  # expected: ends with "xtask check: ok"
  ```

- [x] Commit: `feat(xtask): derive PartialEq on every generated type`

### Task 2: The protocol crate and the event schema

Files: created `docs/schemas/event.schema.json`, `crates/protocol/Cargo.toml`, `crates/protocol/src/lib.rs`, `crates/protocol/src/generated/mod.rs`, `crates/protocol/src/generated/event.rs` (generated), `crates/protocol/src/generated/event.schema.json` (generated); modified `Cargo.toml`, `Cargo.lock`, `xtask/src/generate.rs`; tested by `crates/protocol/src/lib.rs`

Consumes: `cargo xtask generate` from Task 1
Produces: the crate `farik-protocol`; `farik_protocol::generated::event::{EventKind, ContractSummary, TaskCreatedBody, RequestTriagedBody, ContractWrittenBody, ContractLockedBody, ContractUnlockedBody, DriftDetectedBody, ProjectScannedBody, TeamUpdatedBody, CriteriaUpdatedBody, ContractSummaryKind, ContractSummaryParent, ContractSummaryRisk, ContractSummaryStatus, DriftDetectedBodyDrift, RequestTriagedBodySize, FarikEvent, EventBodyWire}`

- [x] Scaffold the crate so that there is something to run a test in. Add to the root `Cargo.toml`, in `[workspace.dependencies]`, between `chrono` and `globset`:

  ```toml
  farik-core = { path = "crates/core" }
  ```

  Create `crates/protocol/Cargo.toml`:

  ```toml
  [package]
  name = "farik-protocol"
  description = "The event envelope, the event kinds, the commands, and the clock and identifier traits shared by the daemon and the front end."
  version.workspace = true
  edition.workspace = true
  license.workspace = true
  repository.workspace = true
  rust-version.workspace = true

  [dependencies]
  chrono.workspace = true
  farik-core.workspace = true
  jsonschema.workspace = true
  regress.workspace = true
  serde.workspace = true
  serde_json.workspace = true

  [lints]
  workspace = true
  ```

  Create `crates/protocol/src/lib.rs` with the crate doc and nothing else yet:

  ```rust
  //! Farik's wire types: the event envelope, the event kinds, the commands, and the traits that
  //! keep the machine's clock and its identifiers out of the crates that decide things.
  ```

  Confirm it builds:

  ```
  cargo build -p farik-protocol
  # expected: Compiling farik-protocol v0.0.0 ... Finished
  ```

- [x] Write the failing test. Append to `crates/protocol/src/lib.rs`:

  ```rust
  #[cfg(test)]
  mod tests {
      use crate::generated::event::EventKind;

      /// Every kind the log holds in this phase, with the wire name the schema gives it.
      const KINDS: [(&str, EventKind); 9] = [
          ("task.created", EventKind::TaskCreated),
          ("request.triaged", EventKind::RequestTriaged),
          ("contract.written", EventKind::ContractWritten),
          ("contract.locked", EventKind::ContractLocked),
          ("contract.unlocked", EventKind::ContractUnlocked),
          ("drift.detected", EventKind::DriftDetected),
          ("project.scanned", EventKind::ProjectScanned),
          ("team.updated", EventKind::TeamUpdated),
          ("criteria.updated", EventKind::CriteriaUpdated),
      ];

      #[test]
      fn names_every_event_kind_as_an_entity_and_a_past_tense_verb() {
          for (wire, kind) in KINDS {
              assert_eq!(kind.to_string(), wire);
              assert_eq!(wire.parse::<EventKind>().expect("a known kind"), kind);
          }
      }
  }
  ```

- [x] Run it and confirm it fails because the module is missing:

  ```
  cargo test -p farik-protocol
  # expected: FAIL to compile, error[E0433]: cannot find `generated` in `crate`, labelled
  #           "could not find `generated` in the crate root"
  ```

- [x] Write the schema. Create `docs/schemas/event.schema.json`:

  ```json
  {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://farik.dev/schemas/event.schema.json",
    "title": "Farik Event",
    "description": "One record in Farik's append-only log (docs/SPEC.md section 8.5). The envelope says where the event belongs and when it was recorded; kind says what happened and body carries that kind's payload. The pairing of kind and body is checked by the reader, farik_protocol::event::event_from_value, and not here: a schema that pairs them with if and then cannot be turned into Rust types by the generator.",
    "type": "object",
    "additionalProperties": false,
    "required": [
      "seq",
      "recorded_at",
      "team_id",
      "project_id",
      "kind",
      "body"
    ],
    "properties": {
      "seq": {
        "type": "integer",
        "minimum": 0,
        "description": "The event's place in the log. Assigned by the store on append and never reused."
      },
      "recorded_at": {
        "type": "string",
        "format": "date-time",
        "description": "When the event was recorded, from the injected clock. Never read from the machine's clock by the crate that builds the event."
      },
      "team_id": {
        "type": "string",
        "description": "The team the event belongs to. A blank one is refused by the reader."
      },
      "project_id": {
        "type": "string",
        "description": "The project the event belongs to. A blank one is refused by the reader."
      },
      "task_id": {
        "type": "string",
        "pattern": "^FRK-[0-9]{1,6}$",
        "description": "The contract the event is about, when it is about one."
      },
      "agent_id": {
        "type": "string",
        "description": "The agent whose work produced the event, when an agent did."
      },
      "session_id": {
        "type": "string",
        "description": "The session the event was recorded in, when it was recorded in one."
      },
      "kind": {
        "$ref": "#/$defs/eventKind"
      },
      "body": {
        "$ref": "#/$defs/eventBodyWire"
      }
    },
    "$defs": {
      "eventKind": {
        "type": "string",
        "enum": [
          "task.created",
          "request.triaged",
          "contract.written",
          "contract.locked",
          "contract.unlocked",
          "drift.detected",
          "project.scanned",
          "team.updated",
          "criteria.updated"
        ]
      },
      "eventBodyWire": {
        "title": "Event Body Wire",
        "description": "Every body shape the log holds. The branches have distinct required properties and none accepts an unknown one, so a body matches exactly one. Which one it must be is decided by kind, in the reader.",
        "oneOf": [
          { "$ref": "#/$defs/taskCreatedBody" },
          { "$ref": "#/$defs/requestTriagedBody" },
          { "$ref": "#/$defs/contractWrittenBody" },
          { "$ref": "#/$defs/contractLockedBody" },
          { "$ref": "#/$defs/contractUnlockedBody" },
          { "$ref": "#/$defs/driftDetectedBody" },
          { "$ref": "#/$defs/projectScannedBody" },
          { "$ref": "#/$defs/teamUpdatedBody" },
          { "$ref": "#/$defs/criteriaUpdatedBody" }
        ]
      },
      "contractSummary": {
        "title": "Contract Summary",
        "description": "The fields of a contract the board shows, repeated on every event that writes one so that the projections can be rebuilt from the log alone (docs/SPEC.md section 8.4). The vocabularies match task-contract.schema.json, which is the source of truth for them; a test in farik-protocol fails when the two drift.",
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "title", "status", "risk"],
        "properties": {
          "kind": { "type": "string", "enum": ["epic", "task"] },
          "parent": { "type": "string", "pattern": "^FRK-[0-9]{1,6}$" },
          "title": { "type": "string" },
          "status": {
            "type": "string",
            "enum": ["draft", "refining", "ready", "assigned", "in_progress", "blocked", "verifying", "rejected", "accepted", "escalated", "cancelled"]
          },
          "risk": { "type": "string", "enum": ["low", "medium", "high"] }
        }
      },
      "taskCreatedBody": {
        "title": "Task Created Body",
        "description": "A request was filed as a draft contract (docs/SPEC.md section 5.16 item 1).",
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "created_by"],
        "properties": {
          "summary": { "$ref": "#/$defs/contractSummary" },
          "created_by": { "type": "string", "description": "The agent id that filed the request, or human." }
        }
      },
      "requestTriagedBody": {
        "title": "Request Triaged Body",
        "description": "Triage sized a request (docs/SPEC.md section 5.16).",
        "type": "object",
        "additionalProperties": false,
        "required": ["size", "reason", "triaged_by"],
        "properties": {
          "size": { "type": "string", "enum": ["large", "small"] },
          "reason": { "type": "string" },
          "triaged_by": { "type": "string" }
        }
      },
      "contractWrittenBody": {
        "title": "Contract Written Body",
        "description": "A contract's content was written or changed.",
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "written_by"],
        "properties": {
          "summary": { "$ref": "#/$defs/contractSummary" },
          "written_by": { "type": "string" }
        }
      },
      "contractLockedBody": {
        "title": "Contract Locked Body",
        "description": "A human took ownership of a contract (docs/SPEC.md section 5.11).",
        "type": "object",
        "additionalProperties": false,
        "required": ["locked_by"],
        "properties": { "locked_by": { "type": "string" } }
      },
      "contractUnlockedBody": {
        "title": "Contract Unlocked Body",
        "description": "A human gave a contract back to the team (docs/SPEC.md section 5.11).",
        "type": "object",
        "additionalProperties": false,
        "required": ["unlocked_by"],
        "properties": { "unlocked_by": { "type": "string" } }
      },
      "driftDetectedBody": {
        "title": "Drift Detected Body",
        "description": "Reconciliation found the files and the log disagreeing (docs/SPEC.md section 8.4).",
        "type": "object",
        "additionalProperties": false,
        "required": ["drift", "detail"],
        "properties": {
          "drift": { "type": "string", "enum": ["contract_without_events", "events_without_contract", "status_mismatch"] },
          "detail": { "type": "string" }
        }
      },
      "projectScannedBody": {
        "title": "Project Scanned Body",
        "description": "The project scan read the repository back to the user and proposed criteria for the library (docs/SPEC.md section 5.13).",
        "type": "object",
        "additionalProperties": false,
        "required": ["read_back", "detected_criteria"],
        "properties": {
          "read_back": { "type": "string" },
          "detected_criteria": { "type": "array", "items": { "type": "string" } }
        }
      },
      "teamUpdatedBody": {
        "title": "Team Updated Body",
        "description": "The team file was written.",
        "type": "object",
        "additionalProperties": false,
        "required": ["team_name", "agent_ids", "updated_by"],
        "properties": {
          "team_name": { "type": "string" },
          "agent_ids": { "type": "array", "items": { "type": "string" } },
          "updated_by": { "type": "string" }
        }
      },
      "criteriaUpdatedBody": {
        "title": "Criteria Updated Body",
        "description": "The criterion library was written (docs/SPEC.md section 5.13).",
        "type": "object",
        "additionalProperties": false,
        "required": ["criterion_names", "updated_by"],
        "properties": {
          "criterion_names": { "type": "array", "items": { "type": "string" } },
          "updated_by": { "type": "string" }
        }
      }
    }
  }
  ```

- [x] Wire the schema into the generator. In `xtask/src/generate.rs`, change the array's length to 3 and append the entry:

  ```rust
      GeneratedSchema {
          schema: "docs/schemas/event.schema.json",
          types: "crates/protocol/src/generated/event.rs",
          schema_copy: "crates/protocol/src/generated/event.schema.json",
      },
  ```

  Create `crates/protocol/src/generated/mod.rs`, without the `command` line, which Task 8 adds:

  ```rust
  //! Rust types generated from the JSON Schemas in `docs/schemas/`. Regenerate with
  //! `cargo xtask generate`.
  //!
  //! The generated types do not enforce every schema rule: `body` here is an untagged enum that
  //! answers "which body is this?" by shape, and the event's `kind` is what decides it. A value is
  //! validated against the schema and then read by kind: `crate::event::event_from_value`.

  pub mod event;
  ```

  Declare the module in `crates/protocol/src/lib.rs`, above the test module:

  ```rust
  /// Types generated from `docs/schemas/`.
  pub mod generated;
  ```

  Generate:

  ```
  cargo xtask generate
  # expected: generated crates/protocol/src/generated/event.rs
  #           generated crates/protocol/src/generated/event.schema.json
  ```

- [x] Run the test; confirm green:

  ```
  cargo test -p farik-protocol
  # expected: test tests::names_every_event_kind_as_an_entity_and_a_past_tense_verb ... ok
  ```

- [x] Commit: `feat(protocol): generate the event types from the event schema`

### Task 3: The spec names every kind this phase emits

Files: modified `docs/SPEC.md`

Consumes: the kind list from Task 2
Produces: section 8.5 lists the nine kinds of this phase

This task changes documentation and has no test cycle; the check that matters is that the list in the spec and the list in the schema agree, which the command below and the reader of the pull request confirm. The replacement prose is quoted below as a blockquote; the `> ` marker is this plan's, not part of the text to write.

- [x] In `docs/SPEC.md` section 8.5, replace the sentence that begins "Kinds are named" with:

  > Kinds are named `<entity>.<past_tense_verb>` (see `docs/standards/code.md`) and include `task.transitioned`, `tool.called`, `tool.returned`, `tool.denied`, `message.posted`, `cost.recorded`, `budget.exhausted`, `escalation.raised`, `escalation.resolved`, `session.started`, `session.ended`, `review.recorded`, `human.accepted`, and, added in 0.2, `question.asked`, `question.answered`, `contract.locked`, `contract.unlocked`, `task.integrated`, `memory.written`, and, added in 0.3, `product_doc.written`, `request.triaged`, and, added in 0.4, `task.created`, `contract.written`, `drift.detected`, `project.scanned`, `team.updated`, `criteria.updated`.

- [x] At the end of the version paragraph at the top of `docs/SPEC.md`, append:

  > Revision 0.4 (2026-09-17) names in section 8.5 the six event kinds phase 2 emits that earlier revisions left unlisted.

- [x] Confirm the two lists agree:

  ```
  grep -o '"[a-z_]*\.[a-z_]*"' docs/schemas/event.schema.json | sort -u
  # expected, one per line: "contract.locked" "contract.unlocked" "contract.written"
  #           "criteria.updated" "drift.detected" "project.scanned" "request.triaged"
  #           "task.created" "team.updated" -- each of which appears in the sentence above
  ```

- [x] Commit: `docs(docs): name every event kind phase 2 emits in section 8.5`

### Task 4: The contract vocabulary is not duplicated silently

Files: modified `crates/core/src/contract.rs`, `crates/protocol/src/lib.rs`; tested by `crates/protocol/src/lib.rs`

Consumes: `crates/protocol/src/generated/event.schema.json` from Task 2
Produces: `farik_core::contract::SCHEMA_JSON`

- [x] Write the failing test. Append to the `tests` module of `crates/protocol/src/lib.rs`:

  ```rust
      #[test]
      fn keeps_the_summary_vocabularies_the_contract_schema_owns() {
          // One schema never references another, so the event schema repeats the contract's kind,
          // status, and risk lists. This is what stops the copy from drifting from the original.
          let event: serde_json::Value =
              serde_json::from_str(include_str!("generated/event.schema.json"))
                  .expect("the embedded event schema is valid JSON");
          let contract: serde_json::Value = serde_json::from_str(farik_core::contract::SCHEMA_JSON)
              .expect("the embedded contract schema is valid JSON");
          let summary = &event["$defs"]["contractSummary"]["properties"];
          let fields = &contract["properties"];
          assert_eq!(summary["kind"]["enum"], fields["kind"]["enum"]);
          assert_eq!(summary["status"]["enum"], fields["status"]["enum"]);
          assert_eq!(summary["risk"]["enum"], fields["risk"]["enum"]);
          assert_eq!(summary["parent"]["pattern"], fields["parent"]["pattern"]);
      }
  ```

- [x] Run it and confirm it fails because the contract schema is not readable from outside its crate:

  ```
  cargo test -p farik-protocol
  # expected: FAIL to compile, error[E0603]: constant `SCHEMA_JSON` is private
  ```

- [x] Write the minimal implementation. In `crates/core/src/contract.rs`, replace the line

  ```rust
  const SCHEMA_JSON: &str = include_str!("generated/task_contract.schema.json");
  ```

  with

  ```rust
  /// The contract schema this crate validates against, embedded at compile time. Public so that a
  /// crate whose own schema repeats one of the contract's vocabularies can test that it still
  /// matches, one schema never being allowed to reference another.
  pub const SCHEMA_JSON: &str = include_str!("generated/task_contract.schema.json");
  ```

- [x] Run the test and both crates' suites; confirm green:

  ```
  cargo test -p farik-protocol -p farik-core
  # expected: all passing, including keeps_the_summary_vocabularies_the_contract_schema_owns
  ```

- [x] Commit: `feat(core): expose the contract schema to another crate's tests`

  The type is `feat(core)` rather than `test(protocol)` because the change that makes the test
  possible is a widening of `farik-core`'s public interface, which is the half of the diff a
  reader of the history needs to find.

### Task 5: The envelope, the bodies, and the reader

Files: created `crates/protocol/src/event.rs`, `crates/protocol/src/event/fixtures.rs`; modified `crates/protocol/src/lib.rs`; tested by `crates/protocol/src/event.rs`

Consumes: `farik_protocol::generated::event::*` from Task 2; `farik_core::contract::{TaskId, ValidationError}` on `main`
Produces: `farik_protocol::event::{EventEnvelope, EventBody, FarikEvent, EventKind, EVERY_KIND, event_from_value}` and `farik_protocol::event::fixtures::{an_event_wire, a_full_event_wire, a_body_wire, a_contract_summary_wire}`

- [x] Write the fixtures first; they are test data, not behavior, and every test below reads them. Create `crates/protocol/src/event/fixtures.rs`:

  ```rust
  //! Builders for test events, usable by every crate's tests.

  use serde_json::{Value, json};

  use crate::event::EventKind;

  /// A schema-valid wire event of one kind, with that kind's body and no optional envelope field.
  #[must_use]
  pub fn an_event_wire(kind: EventKind) -> Value {
      json!({
          "seq": 1,
          "recorded_at": "2026-09-17T10:00:00Z",
          "team_id": "farik",
          "project_id": "farik",
          "kind": kind.to_string(),
          "body": a_body_wire(kind)
      })
  }

  /// The same event with every optional envelope field present.
  #[must_use]
  pub fn a_full_event_wire(kind: EventKind) -> Value {
      let mut event = an_event_wire(kind);
      event["task_id"] = json!("FRK-1");
      event["agent_id"] = json!("maya-chen");
      event["session_id"] = json!("session-1");
      event
  }

  /// The body one kind carries, schema-valid and with no optional field.
  #[must_use]
  pub fn a_body_wire(kind: EventKind) -> Value {
      match kind {
          EventKind::TaskCreated => {
              json!({ "summary": a_contract_summary_wire(), "created_by": "human" })
          }
          EventKind::RequestTriaged => json!({
              "size": "small",
              "reason": "One deliverable and one reviewer.",
              "triaged_by": "sam-ortiz"
          }),
          EventKind::ContractWritten => {
              json!({ "summary": a_contract_summary_wire(), "written_by": "maya-chen" })
          }
          EventKind::ContractLocked => json!({ "locked_by": "human" }),
          EventKind::ContractUnlocked => json!({ "unlocked_by": "human" }),
          EventKind::DriftDetected => json!({
              "drift": "contract_without_events",
              "detail": "FRK-1 has a contract file and no events."
          }),
          EventKind::ProjectScanned => json!({
              "read_back": "A Rust workspace with one crate and a check command.",
              "detected_criteria": ["cargo xtask check"]
          }),
          EventKind::TeamUpdated => json!({
              "team_name": "Farik",
              "agent_ids": ["maya-chen", "sam-ortiz"],
              "updated_by": "human"
          }),
          EventKind::CriteriaUpdated => json!({
              "criterion_names": ["the check passes"],
              "updated_by": "human"
          }),
      }
  }

  /// A schema-valid contract summary: a task in `draft`, with no parent.
  #[must_use]
  pub fn a_contract_summary_wire() -> Value {
      json!({ "kind": "task", "title": "Add a login page", "status": "draft", "risk": "low" })
  }
  ```

- [x] Write the failing test. Create `crates/protocol/src/event.rs` with the module doc, the fixtures declaration, and the tests, and nothing else yet:

  ```rust
  //! Farik's events: `docs/schemas/event.schema.json` as Rust types, the reader that turns an
  //! untrusted value into one, and the writer that turns one back.

  /// Builders for test events, usable by every crate's tests.
  pub mod fixtures;

  #[cfg(test)]
  mod tests {
      use serde_json::json;

      use super::fixtures::{a_body_wire, a_contract_summary_wire, a_full_event_wire, an_event_wire};
      use super::{EVERY_KIND, EventBody, EventKind, ValidationError, event_from_value};

      fn refusal(input: &serde_json::Value) -> Vec<ValidationError> {
          event_from_value(input).expect_err("expected a refusal")
      }

      #[test]
      fn reads_an_event_of_every_kind_and_gives_the_body_its_own_kind_back() {
          for kind in EVERY_KIND {
              let event = event_from_value(&an_event_wire(kind)).expect("valid");
              assert_eq!(event.body.kind(), kind);
              assert_eq!(event.envelope.seq, 1);
              assert_eq!(event.envelope.team_id, "farik");
              assert!(event.envelope.task_id.is_none());
          }
      }

      #[test]
      fn reads_every_optional_field_of_the_envelope() {
          let event = event_from_value(&a_full_event_wire(EventKind::TaskCreated)).expect("valid");
          assert_eq!(
              event.envelope.task_id.as_ref().map(|id| id.to_string()),
              Some("FRK-1".to_string())
          );
          assert_eq!(event.envelope.agent_id.as_deref(), Some("maya-chen"));
          assert_eq!(event.envelope.session_id.as_deref(), Some("session-1"));
      }

      #[test]
      fn reads_the_summary_a_contract_event_carries() {
          let event = event_from_value(&an_event_wire(EventKind::TaskCreated)).expect("valid");
          let EventBody::TaskCreated(body) = event.body else {
              panic!("a task.created event carries a task.created body");
          };
          assert_eq!(body.created_by, "human");
          assert_eq!(body.summary.title, "Add a login page");
          assert_eq!(body.summary.status.to_string(), "draft");
          assert!(body.summary.parent.is_none());
      }

      #[test]
      fn refuses_a_body_that_belongs_to_another_kind() {
          // The schema cannot pair `kind` with `body`, so this is the reader's rule: without it a
          // contract.locked event could carry a task.created body into the log and every reader
          // after it would have to guess which one to believe.
          for kind in EVERY_KIND {
              let other = if kind == EventKind::TaskCreated {
                  EventKind::ContractLocked
              } else {
                  EventKind::TaskCreated
              };
              let mut input = an_event_wire(kind);
              input["body"] = a_body_wire(other);
              let errors = refusal(&input);
              assert_eq!(errors.len(), 1, "{kind}");
              assert_eq!(errors[0].path, "/body", "{kind}");
              assert!(
                  errors[0]
                      .message
                      .starts_with(&format!("a {kind} event does not carry this body")),
                  "{}",
                  errors[0].message
              );
          }
      }

      #[test]
      fn refuses_a_kind_it_does_not_know() {
          let mut input = an_event_wire(EventKind::TaskCreated);
          input["kind"] = json!("task.exploded");
          let errors = refusal(&input);
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/kind");
      }

      #[test]
      fn refuses_a_value_that_is_not_an_event() {
          let errors = refusal(&json!("not an event"));
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/");
      }

      #[test]
      fn refuses_an_unknown_property() {
          let mut input = an_event_wire(EventKind::TaskCreated);
          input["author"] = json!("someone");
          let errors = refusal(&input);
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/");
      }

      #[test]
      fn refuses_a_task_id_that_is_not_one() {
          let mut input = an_event_wire(EventKind::TaskCreated);
          input["task_id"] = json!("TASK-1");
          let errors = refusal(&input);
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/task_id");
      }

      #[test]
      fn refuses_a_blank_team_id_and_a_blank_project_id() {
          // The schema lets a string be empty; an event nobody can attribute to a team and a project
          // cannot be read back out of the log, so the reader refuses it here.
          for (field, path) in [("team_id", "/team_id"), ("project_id", "/project_id")] {
              let mut input = an_event_wire(EventKind::TaskCreated);
              input[field] = json!("   ");
              let errors = refusal(&input);
              assert_eq!(errors.len(), 1, "{field}");
              assert_eq!(errors[0].path, path);
              assert!(
                  errors[0].message.starts_with("is blank"),
                  "{}",
                  errors[0].message
              );
          }
      }

      #[test]
      fn trims_the_ids_and_forgets_an_optional_one_that_is_blank() {
          let mut input = a_full_event_wire(EventKind::TaskCreated);
          input["team_id"] = json!("  farik  ");
          input["agent_id"] = json!("  maya-chen  ");
          input["session_id"] = json!("   ");
          let event = event_from_value(&input).expect("valid");
          assert_eq!(event.envelope.team_id, "farik");
          assert_eq!(event.envelope.agent_id.as_deref(), Some("maya-chen"));
          assert!(event.envelope.session_id.is_none());
      }

      #[test]
      fn reads_a_summary_that_names_a_parent() {
          let mut input = an_event_wire(EventKind::ContractWritten);
          let mut summary = a_contract_summary_wire();
          summary["parent"] = json!("FRK-3");
          input["body"]["summary"] = summary;
          let event = event_from_value(&input).expect("valid");
          let EventBody::ContractWritten(body) = event.body else {
              panic!("a contract.written event carries a contract.written body");
          };
          assert_eq!(
              body.summary.parent.as_ref().map(|id| id.to_string()),
              Some("FRK-3".to_string())
          );
      }
  }
  ```

  Declare the module in `crates/protocol/src/lib.rs`, above the `///` line that documents `pub mod generated;`:

  ```rust
  /// The event envelope, the event bodies, and the reader and writer of the wire form.
  pub mod event;
  ```

- [x] Run it and confirm it fails because the reader and the types are missing:

  ```
  cargo test -p farik-protocol
  # expected: FAIL to compile, two errors, both because the module has nothing in it yet:
  #           error[E0432]: unresolved import `crate::event::EventKind` at
  #           crates/protocol/src/event/fixtures.rs:5, "no `EventKind` in `event`", and
  #           error[E0432]: unresolved imports `super::EVERY_KIND`, `super::EventBody`,
  #           `super::EventKind`, `super::ValidationError`, `super::event_from_value`
  ```

- [x] Write the minimal implementation. Insert into `crates/protocol/src/event.rs`, between the `//!` module doc and the `///` line that documents `pub mod fixtures;`:

  ```rust
  use std::str::FromStr;
  use std::sync::LazyLock;

  use chrono::{DateTime, Utc};
  use jsonschema::Validator;
  use serde::de::DeserializeOwned;
  use serde_json::Value;

  pub use farik_core::contract::{TaskId, ValidationError};

  pub use crate::generated::event::{
      ContractLockedBody, ContractSummary, ContractSummaryKind, ContractSummaryParent,
      ContractSummaryRisk, ContractSummaryStatus, ContractUnlockedBody, ContractWrittenBody,
      CriteriaUpdatedBody, DriftDetectedBody, DriftDetectedBodyDrift, EventKind, ProjectScannedBody,
      RequestTriagedBody, RequestTriagedBodySize, TaskCreatedBody, TeamUpdatedBody,
  };

  use crate::generated::event::FarikEvent as EventWire;
  ```

  and, after `pub mod fixtures;`:

  ```rust
  const SCHEMA_JSON: &str = include_str!("generated/event.schema.json");

  static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
      let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
          "the embedded event schema is valid JSON: it is a copy of docs/schemas/ written by \
           cargo xtask generate and checked for freshness by cargo xtask check",
      );
      jsonschema::options()
          .should_validate_formats(true)
          .build(&schema)
          .expect(
              "the embedded event schema compiles: it is JSON Schema 2020-12 with no external \
               references, and the generator already parsed it",
          )
  });

  /// Every kind the log holds in this phase, in the order `docs/schemas/event.schema.json` lists
  /// them. The step that adds a kind adds it here.
  pub const EVERY_KIND: [EventKind; 9] = [
      EventKind::TaskCreated,
      EventKind::RequestTriaged,
      EventKind::ContractWritten,
      EventKind::ContractLocked,
      EventKind::ContractUnlocked,
      EventKind::DriftDetected,
      EventKind::ProjectScanned,
      EventKind::TeamUpdated,
      EventKind::CriteriaUpdated,
  ];

  /// Everything one event records except what happened: where it belongs and when it was recorded.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct EventEnvelope {
      /// The event's place in the log, assigned by the store on append.
      pub seq: u64,
      /// When the event was recorded, from the injected clock.
      pub recorded_at: DateTime<Utc>,
      /// The team the event belongs to.
      pub team_id: String,
      /// The project the event belongs to.
      pub project_id: String,
      /// The contract the event is about, when it is about one.
      pub task_id: Option<TaskId>,
      /// The agent whose work produced the event, when an agent did.
      pub agent_id: Option<String>,
      /// The session the event was recorded in, when it was recorded in one.
      pub session_id: Option<String>,
  }

  /// What happened: one variant per event kind, each holding that kind's body.
  #[derive(Debug, Clone, PartialEq)]
  pub enum EventBody {
      /// A request was filed as a draft contract.
      TaskCreated(TaskCreatedBody),
      /// Triage sized a request.
      RequestTriaged(RequestTriagedBody),
      /// A contract's content was written or changed.
      ContractWritten(ContractWrittenBody),
      /// A human took ownership of a contract.
      ContractLocked(ContractLockedBody),
      /// A human gave a contract back to the team.
      ContractUnlocked(ContractUnlockedBody),
      /// Reconciliation found the files and the log disagreeing.
      DriftDetected(DriftDetectedBody),
      /// The project scan read the repository back to the user.
      ProjectScanned(ProjectScannedBody),
      /// The team file was written.
      TeamUpdated(TeamUpdatedBody),
      /// The criterion library was written.
      CriteriaUpdated(CriteriaUpdatedBody),
  }

  impl EventBody {
      /// The kind of event this body belongs to.
      #[must_use]
      pub fn kind(&self) -> EventKind {
          match self {
              Self::TaskCreated(_) => EventKind::TaskCreated,
              Self::RequestTriaged(_) => EventKind::RequestTriaged,
              Self::ContractWritten(_) => EventKind::ContractWritten,
              Self::ContractLocked(_) => EventKind::ContractLocked,
              Self::ContractUnlocked(_) => EventKind::ContractUnlocked,
              Self::DriftDetected(_) => EventKind::DriftDetected,
              Self::ProjectScanned(_) => EventKind::ProjectScanned,
              Self::TeamUpdated(_) => EventKind::TeamUpdated,
              Self::CriteriaUpdated(_) => EventKind::CriteriaUpdated,
          }
      }
  }

  /// One record of the log.
  #[derive(Debug, Clone, PartialEq)]
  pub struct FarikEvent {
      /// Where the event belongs and when it was recorded.
      pub envelope: EventEnvelope,
      /// What happened.
      pub body: EventBody,
  }

  /// Checks a value against `docs/schemas/event.schema.json` and, when it conforms, returns the
  /// typed event. Refuses anything the schema refuses; refuses a body that does not belong to the
  /// event's `kind`, which the schema cannot say; and refuses a blank `team_id` or `project_id`,
  /// which name nobody. Trims every id and drops an optional one left blank.
  ///
  /// # Errors
  ///
  /// Every schema violation, in the schema's order rather than the input's key order; one error at
  /// `/body` when the body does not fit the kind; one error at `/team_id` or `/project_id` for a
  /// blank id; one at `/task_id` for an id the contract's pattern refuses; or one error at the root
  /// when the schema passes but the typed event cannot be built.
  pub fn event_from_value(input: &Value) -> Result<FarikEvent, Vec<ValidationError>> {
      let errors: Vec<ValidationError> = VALIDATOR
          .iter_errors(input)
          .map(|error| ValidationError {
              path: pointer(&error.instance_path().to_string()),
              message: error.to_string(),
          })
          .collect();
      if !errors.is_empty() {
          return Err(errors);
      }
      let wire = serde_json::from_value::<EventWire>(input.clone()).map_err(|error| {
          vec![ValidationError {
              path: "/".to_string(),
              message: format!("the schema passed but the typed event could not be built: {error}"),
          }]
      })?;
      let envelope = envelope_from_wire(&wire)?;
      let body = body_from_value(wire.kind, &input["body"])?;
      Ok(FarikEvent { envelope, body })
  }

  fn envelope_from_wire(wire: &EventWire) -> Result<EventEnvelope, Vec<ValidationError>> {
      let team_id = required_id(&wire.team_id, "/team_id")?;
      let project_id = required_id(&wire.project_id, "/project_id")?;
      let task_id = match &wire.task_id {
          None => None,
          Some(id) => Some(TaskId::from_str(id.as_str()).map_err(|error| {
              vec![ValidationError {
                  path: "/task_id".to_string(),
                  message: error.to_string(),
              }]
          })?),
      };
      Ok(EventEnvelope {
          seq: wire.seq,
          recorded_at: wire.recorded_at,
          team_id,
          project_id,
          task_id,
          agent_id: optional_id(wire.agent_id.as_deref()),
          session_id: optional_id(wire.session_id.as_deref()),
      })
  }

  /// The body the kind says it is, read from the value. The wire enum answers "which body is this?"
  /// by shape; the kind is what the event says it is, so the body is read by kind and one that does
  /// not fit is refused.
  fn body_from_value(kind: EventKind, body: &Value) -> Result<EventBody, Vec<ValidationError>> {
      Ok(match kind {
          EventKind::TaskCreated => EventBody::TaskCreated(read_body(body, kind)?),
          EventKind::RequestTriaged => EventBody::RequestTriaged(read_body(body, kind)?),
          EventKind::ContractWritten => EventBody::ContractWritten(read_body(body, kind)?),
          EventKind::ContractLocked => EventBody::ContractLocked(read_body(body, kind)?),
          EventKind::ContractUnlocked => EventBody::ContractUnlocked(read_body(body, kind)?),
          EventKind::DriftDetected => EventBody::DriftDetected(read_body(body, kind)?),
          EventKind::ProjectScanned => EventBody::ProjectScanned(read_body(body, kind)?),
          EventKind::TeamUpdated => EventBody::TeamUpdated(read_body(body, kind)?),
          EventKind::CriteriaUpdated => EventBody::CriteriaUpdated(read_body(body, kind)?),
      })
  }

  fn read_body<Body: DeserializeOwned>(
      body: &Value,
      kind: EventKind,
  ) -> Result<Body, Vec<ValidationError>> {
      serde_json::from_value::<Body>(body.clone()).map_err(|error| {
          vec![ValidationError {
              path: "/body".to_string(),
              message: format!("a {kind} event does not carry this body: {error}"),
          }]
      })
  }

  fn required_id(value: &str, path: &str) -> Result<String, Vec<ValidationError>> {
      let trimmed = value.trim();
      if trimmed.is_empty() {
          return Err(vec![ValidationError {
              path: path.to_string(),
              message: "is blank, and an event the log cannot attribute to a team and a project \
                        cannot be read back"
                  .to_string(),
          }]);
      }
      Ok(trimmed.to_string())
  }

  fn optional_id(value: Option<&str>) -> Option<String> {
      value
          .map(str::trim)
          .filter(|named| !named.is_empty())
          .map(ToString::to_string)
  }

  fn pointer(path: &str) -> String {
      if path.is_empty() {
          "/".to_string()
      } else {
          path.to_string()
      }
  }
  ```

- [x] Run the test and the crate's suite; confirm green:

  ```
  cargo test -p farik-protocol
  # expected: all passing, eleven tests in event::tests
  ```

- [x] Tie the two lists of kinds together so that neither can gain a kind without the other. Add one line to `names_every_event_kind_as_an_entity_and_a_past_tense_verb` in `crates/protocol/src/lib.rs`, as the function's last statement, after the `for` loop:

  ```rust
          assert_eq!(KINDS.map(|(_, kind)| kind), crate::event::EVERY_KIND);
  ```

  ```
  cargo test -p farik-protocol
  # expected: still all passing
  ```

- [x] Commit: `feat(protocol): read an event from its wire form`

### Task 6: The writer and the round trip

Files: modified `crates/protocol/src/event.rs`; tested by `crates/protocol/src/event.rs`

Consumes: `event_from_value`, `EVERY_KIND`, the fixtures, from Task 5
Produces: `farik_protocol::event::event_to_value`

- [x] Write the failing test. Append to the `tests` module of `crates/protocol/src/event.rs`:

  ```rust
      #[test]
      fn writes_back_exactly_the_value_it_read_for_every_kind() {
          // The writer is hand-written, so this is what holds it to the schema the reader checks.
          for kind in EVERY_KIND {
              let wire = a_full_event_wire(kind);
              let event = event_from_value(&wire).expect("valid");
              assert_eq!(event_to_value(&event), wire, "{kind}");
          }
      }

      #[test]
      fn writes_a_summary_and_its_parent() {
          let mut input = an_event_wire(EventKind::ContractWritten);
          input["body"]["summary"]["parent"] = json!("FRK-3");
          let event = event_from_value(&input).expect("valid");
          assert_eq!(event_to_value(&event), input);
      }

      #[test]
      fn leaves_out_the_optional_fields_that_are_not_there() {
          let event = event_from_value(&an_event_wire(EventKind::ContractLocked)).expect("valid");
          let wire = event_to_value(&event);
          for absent in ["task_id", "agent_id", "session_id"] {
              assert!(wire.get(absent).is_none(), "{absent}");
          }
      }
  ```

  and add `event_to_value` to that module's `use super::{...}` line, which becomes:

  ```rust
      use super::{
          EVERY_KIND, EventBody, EventKind, ValidationError, event_from_value, event_to_value,
      };
  ```

- [x] Run it and confirm it fails because the writer is missing:

  ```
  cargo test -p farik-protocol
  # expected: FAIL to compile, error[E0432]: unresolved import `super::event_to_value`
  ```

- [x] Write the minimal implementation. Change the two import lines at the top of `crates/protocol/src/event.rs` to

  ```rust
  use chrono::{DateTime, SecondsFormat, Utc};
  ```

  and

  ```rust
  use serde_json::{Map, Value};
  ```

  then insert immediately before `#[cfg(test)]`, which is the last item in the file:

  ```rust
  /// One event as the wire value the log holds, the inverse of `event_from_value`.
  ///
  /// The crate writes the wire form itself, field by field, rather than deriving it: `serde_json`
  /// answers with a `Result` whose error cannot happen for these types, and `docs/standards/code.md`
  /// allows no `unwrap` or `expect` here, so the alternative is an impossible error on every caller
  /// forever. The round-trip test is what keeps this honest.
  #[must_use]
  pub fn event_to_value(event: &FarikEvent) -> Value {
      let mut wire = Map::new();
      wire.insert("seq".to_string(), Value::from(event.envelope.seq));
      wire.insert(
          "recorded_at".to_string(),
          Value::String(
              event
                  .envelope
                  .recorded_at
                  .to_rfc3339_opts(SecondsFormat::AutoSi, true),
          ),
      );
      wire.insert(
          "team_id".to_string(),
          Value::String(event.envelope.team_id.clone()),
      );
      wire.insert(
          "project_id".to_string(),
          Value::String(event.envelope.project_id.clone()),
      );
      if let Some(task_id) = &event.envelope.task_id {
          wire.insert("task_id".to_string(), Value::String(task_id.to_string()));
      }
      if let Some(agent_id) = &event.envelope.agent_id {
          wire.insert("agent_id".to_string(), Value::String(agent_id.clone()));
      }
      if let Some(session_id) = &event.envelope.session_id {
          wire.insert("session_id".to_string(), Value::String(session_id.clone()));
      }
      wire.insert(
          "kind".to_string(),
          Value::String(event.body.kind().to_string()),
      );
      wire.insert("body".to_string(), body_to_value(&event.body));
      Value::Object(wire)
  }

  fn body_to_value(body: &EventBody) -> Value {
      let mut wire = Map::new();
      match body {
          EventBody::TaskCreated(body) => {
              wire.insert("summary".to_string(), summary_to_value(&body.summary));
              wire.insert(
                  "created_by".to_string(),
                  Value::String(body.created_by.clone()),
              );
          }
          EventBody::RequestTriaged(body) => {
              wire.insert("size".to_string(), Value::String(body.size.to_string()));
              wire.insert("reason".to_string(), Value::String(body.reason.clone()));
              wire.insert(
                  "triaged_by".to_string(),
                  Value::String(body.triaged_by.clone()),
              );
          }
          EventBody::ContractWritten(body) => {
              wire.insert("summary".to_string(), summary_to_value(&body.summary));
              wire.insert(
                  "written_by".to_string(),
                  Value::String(body.written_by.clone()),
              );
          }
          EventBody::ContractLocked(body) => {
              wire.insert(
                  "locked_by".to_string(),
                  Value::String(body.locked_by.clone()),
              );
          }
          EventBody::ContractUnlocked(body) => {
              wire.insert(
                  "unlocked_by".to_string(),
                  Value::String(body.unlocked_by.clone()),
              );
          }
          EventBody::DriftDetected(body) => {
              wire.insert("drift".to_string(), Value::String(body.drift.to_string()));
              wire.insert("detail".to_string(), Value::String(body.detail.clone()));
          }
          EventBody::ProjectScanned(body) => {
              wire.insert(
                  "read_back".to_string(),
                  Value::String(body.read_back.clone()),
              );
              wire.insert(
                  "detected_criteria".to_string(),
                  strings(&body.detected_criteria),
              );
          }
          EventBody::TeamUpdated(body) => {
              wire.insert(
                  "team_name".to_string(),
                  Value::String(body.team_name.clone()),
              );
              wire.insert("agent_ids".to_string(), strings(&body.agent_ids));
              wire.insert(
                  "updated_by".to_string(),
                  Value::String(body.updated_by.clone()),
              );
          }
          EventBody::CriteriaUpdated(body) => {
              wire.insert(
                  "criterion_names".to_string(),
                  strings(&body.criterion_names),
              );
              wire.insert(
                  "updated_by".to_string(),
                  Value::String(body.updated_by.clone()),
              );
          }
      }
      Value::Object(wire)
  }

  fn summary_to_value(summary: &ContractSummary) -> Value {
      let mut wire = Map::new();
      wire.insert("kind".to_string(), Value::String(summary.kind.to_string()));
      if let Some(parent) = &summary.parent {
          wire.insert("parent".to_string(), Value::String(parent.to_string()));
      }
      wire.insert("title".to_string(), Value::String(summary.title.clone()));
      wire.insert(
          "status".to_string(),
          Value::String(summary.status.to_string()),
      );
      wire.insert("risk".to_string(), Value::String(summary.risk.to_string()));
      Value::Object(wire)
  }

  fn strings(values: &[String]) -> Value {
      Value::Array(
          values
              .iter()
              .map(|value| Value::String(value.clone()))
              .collect(),
      )
  }
  ```

- [x] Run the test and the crate's suite; confirm green:

  ```
  cargo test -p farik-protocol
  # expected: all passing, including writes_back_exactly_the_value_it_read_for_every_kind
  ```

- [x] Commit: `feat(protocol): write an event back to its wire form`

### Task 7: An event before it has a sequence number

Files: modified `crates/protocol/src/event.rs`; tested by `crates/protocol/src/event.rs`

Consumes: `EventBody`, `TaskId`, from Task 5
Produces: `farik_protocol::event::{NewEvent, EventIds, EventError, new_event}`

- [x] Write the failing test. Append to the `tests` module of `crates/protocol/src/event.rs`:

  ```rust
      fn some_ids() -> EventIds {
          EventIds {
              team_id: "farik".to_string(),
              project_id: "farik".to_string(),
              task_id: None,
              agent_id: None,
              session_id: None,
          }
      }

      fn a_body() -> EventBody {
          let event = event_from_value(&an_event_wire(EventKind::ContractLocked)).expect("valid");
          event.body
      }

      fn at() -> chrono::DateTime<chrono::Utc> {
          let event = event_from_value(&an_event_wire(EventKind::ContractLocked)).expect("valid");
          event.envelope.recorded_at
      }

      #[test]
      fn stamps_a_body_with_the_time_and_the_ids_it_belongs_to() {
          let new = new_event(a_body(), at(), some_ids()).expect("stamped");
          assert_eq!(new.team_id, "farik");
          assert_eq!(new.project_id, "farik");
          assert_eq!(new.recorded_at, at());
          assert_eq!(new.body.kind(), EventKind::ContractLocked);
          assert!(new.task_id.is_none());
      }

      #[test]
      fn trims_every_id_and_forgets_an_optional_one_that_is_blank() {
          let ids = EventIds {
              team_id: "  farik  ".to_string(),
              project_id: "  farik  ".to_string(),
              agent_id: Some("  maya-chen  ".to_string()),
              session_id: Some("   ".to_string()),
              ..some_ids()
          };
          let new = new_event(a_body(), at(), ids).expect("stamped");
          assert_eq!(new.team_id, "farik");
          assert_eq!(new.project_id, "farik");
          assert_eq!(new.agent_id.as_deref(), Some("maya-chen"));
          assert!(new.session_id.is_none());
      }

      #[test]
      fn refuses_to_stamp_an_event_with_a_blank_team_id_or_project_id() {
          // A blank one names nobody, and the log would hold a record that cannot be attributed or
          // read back. The reader refuses the same thing; this is the other door into the log.
          for (field, ids) in [
              (
                  "team_id",
                  EventIds {
                      team_id: "   ".to_string(),
                      ..some_ids()
                  },
              ),
              (
                  "project_id",
                  EventIds {
                      project_id: String::new(),
                      ..some_ids()
                  },
              ),
          ] {
              let error = new_event(a_body(), at(), ids).expect_err("expected a refusal");
              assert_eq!(
                  error,
                  EventError::BlankId {
                      field: field.to_string()
                  }
              );
          }
      }
  ```

  and extend that module's `use super::{...}` line, which becomes:

  ```rust
      use super::{
          EVERY_KIND, EventBody, EventError, EventIds, EventKind, ValidationError, event_from_value,
          event_to_value, new_event,
      };
  ```

- [x] Run it and confirm it fails because nothing stamps an event yet:

  ```
  cargo test -p farik-protocol
  # expected: FAIL to compile, error[E0432]: unresolved imports `super::EventError`,
  #           `super::EventIds`, `super::new_event`
  ```

- [x] Write the minimal implementation. Insert into `crates/protocol/src/event.rs`, after `FarikEvent` and before `event_from_value`:

  ```rust
  /// The ids an event is stamped with. Everything an envelope has except the sequence number, which
  /// the store assigns, and the time, which the clock does.
  #[derive(Debug, Clone, PartialEq, Eq, Default)]
  pub struct EventIds {
      /// The team the event belongs to. Blank is refused.
      pub team_id: String,
      /// The project the event belongs to. Blank is refused.
      pub project_id: String,
      /// The contract the event is about, when it is about one.
      pub task_id: Option<TaskId>,
      /// The agent whose work produced the event, when an agent did.
      pub agent_id: Option<String>,
      /// The session the event was recorded in, when it was recorded in one.
      pub session_id: Option<String>,
  }

  /// An event that has not been appended yet: everything a `FarikEvent` has except the sequence
  /// number, which the store assigns on append.
  #[derive(Debug, Clone, PartialEq)]
  pub struct NewEvent {
      /// When the event was recorded, from the injected clock.
      pub recorded_at: DateTime<Utc>,
      /// The team the event belongs to.
      pub team_id: String,
      /// The project the event belongs to.
      pub project_id: String,
      /// The contract the event is about, when it is about one.
      pub task_id: Option<TaskId>,
      /// The agent whose work produced the event, when an agent did.
      pub agent_id: Option<String>,
      /// The session the event was recorded in, when it was recorded in one.
      pub session_id: Option<String>,
      /// What happened.
      pub body: EventBody,
  }

  /// Why an event cannot be stamped.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum EventError {
      /// An id that must name something was blank once trimmed.
      BlankId {
          /// The field: `team_id` or `project_id`.
          field: String,
      },
  }

  /// Stamps a body with the time it was recorded and the ids it belongs to. Trims every id and drops
  /// an optional one that is blank, because a blank id names nobody and the log would show an agent
  /// or a session that does not exist.
  ///
  /// # Errors
  ///
  /// `BlankId` when `team_id` or `project_id` is blank once trimmed; `team_id` is reported first.
  pub fn new_event(
      body: EventBody,
      recorded_at: DateTime<Utc>,
      ids: EventIds,
  ) -> Result<NewEvent, EventError> {
      let team_id = named(&ids.team_id, "team_id")?;
      let project_id = named(&ids.project_id, "project_id")?;
      Ok(NewEvent {
          recorded_at,
          team_id,
          project_id,
          task_id: ids.task_id,
          agent_id: optional_id(ids.agent_id.as_deref()),
          session_id: optional_id(ids.session_id.as_deref()),
          body,
      })
  }

  fn named(value: &str, field: &str) -> Result<String, EventError> {
      let trimmed = value.trim();
      if trimmed.is_empty() {
          return Err(EventError::BlankId {
              field: field.to_string(),
          });
      }
      Ok(trimmed.to_string())
  }
  ```

- [x] Run the test and the crate's suite; confirm green:

  ```
  cargo test -p farik-protocol
  # expected: all passing
  ```

- [x] Commit: `feat(protocol): stamp an event with its time and its ids`

### Task 8: The command schema and the commands

Files: created `docs/schemas/command.schema.json`, `crates/protocol/src/command.rs`, `crates/protocol/src/generated/command.rs` (generated), `crates/protocol/src/generated/command.schema.json` (generated); modified `xtask/src/generate.rs`, `crates/protocol/src/generated/mod.rs`, `crates/protocol/src/lib.rs`; tested by `crates/protocol/src/command.rs`

Consumes: `farik_core::contract::{TaskContract, TaskId, ValidationError, validate_contract}` and `farik_core::contract::fixtures::a_contract_wire` on `main`
Produces: `farik_protocol::command::{Command, CommandName, RequestSize, command_from_value}`

- [ ] Write the schema. Create `docs/schemas/command.schema.json`:

  ```json
  {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "$id": "https://farik.dev/schemas/command.schema.json",
    "title": "Farik Command",
    "description": "A request for the daemon to change something. Shaped like an event: command names what is asked and body carries that command's arguments. The pairing of the two is checked by the reader, farik_protocol::command::command_from_value.",
    "type": "object",
    "additionalProperties": false,
    "required": ["command", "body"],
    "properties": {
      "command": { "$ref": "#/$defs/commandName" },
      "body": { "$ref": "#/$defs/commandBodyWire" }
    },
    "$defs": {
      "commandName": {
        "type": "string",
        "enum": ["task_create", "request_triage"]
      },
      "commandBodyWire": {
        "title": "Command Body Wire",
        "oneOf": [
          { "$ref": "#/$defs/taskCreateBody" },
          { "$ref": "#/$defs/requestTriageBody" }
        ]
      },
      "taskCreateBody": {
        "title": "Task Create Body",
        "type": "object",
        "additionalProperties": false,
        "required": ["contract"],
        "properties": {
          "contract": {
            "type": "object",
            "description": "A task contract. This schema says only that it is an object: one schema never references another, and the contract's rules, the repeated-id ones among them, belong to farik_core::contract::validate_contract, which the reader calls."
          }
        }
      },
      "requestTriageBody": {
        "title": "Request Triage Body",
        "type": "object",
        "additionalProperties": false,
        "required": ["task_id", "size", "reason"],
        "properties": {
          "task_id": { "type": "string", "pattern": "^FRK-[0-9]{1,6}$" },
          "size": { "type": "string", "enum": ["large", "small"] },
          "reason": { "type": "string" }
        }
      }
    }
  }
  ```

  Append the entry to `GENERATED_SCHEMAS` in `xtask/src/generate.rs` and change the array's length to 4:

  ```rust
      GeneratedSchema {
          schema: "docs/schemas/command.schema.json",
          types: "crates/protocol/src/generated/command.rs",
          schema_copy: "crates/protocol/src/generated/command.schema.json",
      },
  ```

  Declare the module in `crates/protocol/src/generated/mod.rs`, above `pub mod event;`, which carries no doc comment of its own:

  ```rust
  pub mod command;
  ```

  Generate:

  ```
  cargo xtask generate
  # expected: generated crates/protocol/src/generated/command.rs
  #           generated crates/protocol/src/generated/command.schema.json
  ```

- [ ] Write the failing test. Create `crates/protocol/src/command.rs` with the module doc and the tests only:

  ```rust
  //! The commands the daemon accepts: `docs/schemas/command.schema.json` as Rust types, and the
  //! reader that turns an untrusted value into one.

  #[cfg(test)]
  mod tests {
      use farik_core::contract::fixtures::a_contract_wire;
      use serde_json::{Value, json};

      use super::{Command, RequestSize, ValidationError, command_from_value};

      fn a_task_create_wire() -> Value {
          json!({ "command": "task_create", "body": { "contract": a_contract_wire() } })
      }

      fn a_request_triage_wire() -> Value {
          json!({
              "command": "request_triage",
              "body": { "task_id": "FRK-1", "size": "large", "reason": "Three deliverables." }
          })
      }

      fn refusal(input: &Value) -> Vec<ValidationError> {
          command_from_value(input).expect_err("expected a refusal")
      }

      #[test]
      fn reads_a_task_create_command_through_the_contract_validator() {
          let Command::TaskCreate { contract } =
              command_from_value(&a_task_create_wire()).expect("valid")
          else {
              panic!("a task_create command carries a contract");
          };
          assert_eq!(contract.id.to_string(), "FRK-1");
          // The validator applied the schema's defaults, which is the proof it was the one used.
          assert_eq!(contract.budget.max_sessions.get(), 5);
      }

      #[test]
      fn reads_a_request_triage_command() {
          let Command::RequestTriage {
              task_id,
              size,
              reason,
          } = command_from_value(&a_request_triage_wire()).expect("valid")
          else {
              panic!("a request_triage command carries a size");
          };
          assert_eq!(task_id.to_string(), "FRK-1");
          assert_eq!(size, RequestSize::Large);
          assert_eq!(reason, "Three deliverables.");
      }

      #[test]
      fn refuses_a_contract_the_contract_schema_refuses_and_says_where() {
          // The repeated criterion id rule of phase 1 lives in validate_contract; a contract that
          // arrives inside a command is held to it too, and its path is reported under the body.
          let mut input = a_task_create_wire();
          let twin = input["body"]["contract"]["exit_criteria"][0].clone();
          input["body"]["contract"]["exit_criteria"] = json!([twin.clone(), twin]);
          let errors = refusal(&input);
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/body/contract/exit_criteria");
          assert!(
              errors[0].message.starts_with("the id C1 names"),
              "{}",
              errors[0].message
          );
      }

      #[test]
      fn reports_a_contract_refused_at_its_root_under_the_body() {
          let mut input = a_task_create_wire();
          input["body"]["contract"]["owner"] = json!("someone");
          let errors = refusal(&input);
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/body/contract");
      }

      #[test]
      fn refuses_a_body_that_belongs_to_another_command() {
          let mut input = a_task_create_wire();
          input["body"] = a_request_triage_wire()["body"].clone();
          let errors = refusal(&input);
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/body");
          assert!(
              errors[0]
                  .message
                  .starts_with("a task_create command does not carry this body"),
              "{}",
              errors[0].message
          );
      }

      #[test]
      fn refuses_a_command_it_does_not_know() {
          let mut input = a_task_create_wire();
          input["command"] = json!("task_delete");
          let errors = refusal(&input);
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/command");
      }

      #[test]
      fn refuses_a_value_that_is_not_a_command() {
          let errors = refusal(&json!(["task_create"]));
          assert_eq!(errors.len(), 1);
          assert_eq!(errors[0].path, "/");
      }
  }
  ```

  Declare the module in `crates/protocol/src/lib.rs`, above the `///` line that documents `pub mod event;`:

  ```rust
  /// The commands the daemon accepts.
  pub mod command;
  ```

- [ ] Run it and confirm it fails because nothing reads a command yet:

  ```
  cargo test -p farik-protocol
  # expected: FAIL to compile, error[E0432]: unresolved imports `super::Command`,
  #           `super::RequestSize`, `super::ValidationError`, `super::command_from_value`
  ```

- [ ] Write the minimal implementation. Insert into `crates/protocol/src/command.rs`, between the module doc and the tests:

  ```rust
  use std::str::FromStr;
  use std::sync::LazyLock;

  use farik_core::contract::validate_contract;
  use jsonschema::Validator;
  use serde::de::DeserializeOwned;
  use serde_json::Value;

  pub use farik_core::contract::{TaskContract, TaskId, ValidationError};

  pub use crate::generated::command::CommandName;
  use crate::generated::command::{
      FarikCommand as CommandWire, RequestTriageBody, RequestTriageBodySize, TaskCreateBody,
  };

  const SCHEMA_JSON: &str = include_str!("generated/command.schema.json");

  static VALIDATOR: LazyLock<Validator> = LazyLock::new(|| {
      let schema: Value = serde_json::from_str(SCHEMA_JSON).expect(
          "the embedded command schema is valid JSON: it is a copy of docs/schemas/ written by \
           cargo xtask generate and checked for freshness by cargo xtask check",
      );
      jsonschema::options()
          .should_validate_formats(true)
          .build(&schema)
          .expect(
              "the embedded command schema compiles: it is JSON Schema 2020-12 with no external \
               references, and the generator already parsed it",
          )
  });

  /// How big triage found a request (`docs/SPEC.md` section 5.16).
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum RequestSize {
      /// Large: the request becomes an epic.
      Large,
      /// Small: the request becomes one standalone task.
      Small,
  }

  /// A request for the daemon to change something.
  #[derive(Debug, Clone, PartialEq)]
  pub enum Command {
      /// File a contract as a draft request. The contract is boxed because it is an order of
      /// magnitude larger than every other command's arguments, and an enum is as large as its
      /// largest variant.
      TaskCreate {
          /// The contract, already held to every rule `validate_contract` applies.
          contract: Box<TaskContract>,
      },
      /// Record triage's decision, or the human's overrule of it.
      RequestTriage {
          /// The request being sized.
          task_id: TaskId,
          /// How big it is.
          size: RequestSize,
          /// Why, in the triager's words.
          reason: String,
      },
  }

  /// Checks a value against `docs/schemas/command.schema.json` and, when it conforms, returns the
  /// typed command. The contract inside `task_create` goes through
  /// `farik_core::contract::validate_contract`, so that a contract arriving inside a command is held
  /// to exactly the rules one arriving alone is, the repeated id rules among them.
  ///
  /// # Errors
  ///
  /// Every schema violation, in the schema's order rather than the input's key order; one error at
  /// `/body` when the body does not belong to the command; every violation the contract's own
  /// validator reports, at its path under `/body/contract`; or one error at the root when the schema
  /// passes but the typed command cannot be built.
  pub fn command_from_value(input: &Value) -> Result<Command, Vec<ValidationError>> {
      let errors: Vec<ValidationError> = VALIDATOR
          .iter_errors(input)
          .map(|error| ValidationError {
              path: pointer(&error.instance_path().to_string()),
              message: error.to_string(),
          })
          .collect();
      if !errors.is_empty() {
          return Err(errors);
      }
      let wire = serde_json::from_value::<CommandWire>(input.clone()).map_err(|error| {
          vec![ValidationError {
              path: "/".to_string(),
              message: format!("the schema passed but the typed command could not be built: {error}"),
          }]
      })?;
      match wire.command {
          CommandName::TaskCreate => {
              let body: TaskCreateBody = read_body(&input["body"], CommandName::TaskCreate)?;
              let contract = validate_contract(&Value::Object(body.contract)).map_err(|errors| {
                  errors
                      .into_iter()
                      .map(|error| ValidationError {
                          path: under_contract(&error.path),
                          message: error.message,
                      })
                      .collect::<Vec<ValidationError>>()
              })?;
              Ok(Command::TaskCreate {
                  contract: Box::new(contract),
              })
          }
          CommandName::RequestTriage => {
              let body: RequestTriageBody = read_body(&input["body"], CommandName::RequestTriage)?;
              let task_id = TaskId::from_str(body.task_id.as_str()).map_err(|error| {
                  vec![ValidationError {
                      path: "/body/task_id".to_string(),
                      message: error.to_string(),
                  }]
              })?;
              Ok(Command::RequestTriage {
                  task_id,
                  size: match body.size {
                      RequestTriageBodySize::Large => RequestSize::Large,
                      RequestTriageBodySize::Small => RequestSize::Small,
                  },
                  reason: body.reason,
              })
          }
      }
  }

  fn read_body<Body: DeserializeOwned>(
      body: &Value,
      command: CommandName,
  ) -> Result<Body, Vec<ValidationError>> {
      serde_json::from_value::<Body>(body.clone()).map_err(|error| {
          vec![ValidationError {
              path: "/body".to_string(),
              message: format!("a {command} command does not carry this body: {error}"),
          }]
      })
  }

  /// A contract's own error path, moved under the command that carried it. The validator reports the
  /// contract's root as `/`, which under a command is the contract itself.
  fn under_contract(path: &str) -> String {
      if path == "/" {
          "/body/contract".to_string()
      } else {
          format!("/body/contract{path}")
      }
  }

  fn pointer(path: &str) -> String {
      if path.is_empty() {
          "/".to_string()
      } else {
          path.to_string()
      }
  }
  ```

- [ ] Run the test and the crate's suite; confirm green:

  ```
  cargo test -p farik-protocol
  # expected: all passing, seven tests in command::tests
  ```

- [ ] Commit: `feat(protocol): read a command from its wire form`

### Task 9: Time and identifiers, injected

Files: created `crates/protocol/src/clock.rs`; modified `crates/protocol/src/lib.rs`; tested by `crates/protocol/src/clock.rs`

Consumes: nothing
Produces: `farik_protocol::clock::{Clock, IdSource, FixedClock, SequentialIds}`

- [ ] Write the failing test. Create `crates/protocol/src/clock.rs` with the module doc and the tests only:

  ```rust
  //! Time and identifiers, injected rather than read from the machine, so that a test decides both
  //! and two runs over the same input produce the same events.

  #[cfg(test)]
  mod tests {
      use chrono::{DateTime, Utc};

      use super::{Clock, FixedClock, IdSource, SequentialIds};

      fn at() -> DateTime<Utc> {
          "2026-09-17T10:00:00Z"
              .parse::<DateTime<Utc>>()
              .expect("a fixed timestamp")
      }

      #[test]
      fn answers_the_same_time_however_often_it_is_asked() {
          let clock = FixedClock::new(at());
          assert_eq!(clock.now(), at());
          assert_eq!(clock.now(), at());
      }

      #[test]
      fn hands_out_a_new_session_id_each_time() {
          let ids = SequentialIds::new();
          assert_eq!(ids.session_id(), "session-1");
          assert_eq!(ids.session_id(), "session-2");
          assert_eq!(ids.session_id(), "session-3");
      }

      #[test]
      fn starts_a_default_source_at_the_first_id() {
          let ids = SequentialIds::default();
          assert_eq!(ids.session_id(), "session-1");
      }
  }
  ```

  Declare the module in `crates/protocol/src/lib.rs`, above the `///` line that documents `pub mod command;`:

  ```rust
  /// Time and identifiers, injected rather than read from the machine.
  pub mod clock;
  ```

- [ ] Run it and confirm it fails because the traits are missing:

  ```
  cargo test -p farik-protocol
  # expected: FAIL to compile, error[E0432]: unresolved imports `super::Clock`,
  #           `super::FixedClock`, `super::IdSource`, `super::SequentialIds`
  ```

- [ ] Write the minimal implementation. Insert into `crates/protocol/src/clock.rs`, between the module doc and the tests:

  ```rust
  use std::sync::atomic::{AtomicU64, Ordering};

  use chrono::{DateTime, Utc};

  /// Where the current time comes from. Nothing below the runtime reads the machine's clock, so that
  /// a test decides what "now" is and a replayed log produces the same answers twice.
  pub trait Clock {
      /// The time now, in UTC.
      fn now(&self) -> DateTime<Utc>;
  }

  /// Where an identifier that Farik cannot derive from what it already has comes from.
  pub trait IdSource {
      /// A new session id.
      fn session_id(&self) -> String;
  }

  /// A clock that always answers the same time. For tests, in this crate and in every other.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct FixedClock {
      /// The time it answers.
      pub at: DateTime<Utc>,
  }

  impl FixedClock {
      /// A clock fixed at `at`.
      #[must_use]
      pub fn new(at: DateTime<Utc>) -> Self {
          Self { at }
      }
  }

  impl Clock for FixedClock {
      fn now(&self) -> DateTime<Utc> {
          self.at
      }
  }

  /// An identifier source that hands out `session-1`, `session-2`, and so on. For tests, in this
  /// crate and in every other.
  #[derive(Debug, Default)]
  pub struct SequentialIds {
      handed_out: AtomicU64,
  }

  impl SequentialIds {
      /// A source whose first identifier is `session-1`.
      #[must_use]
      pub fn new() -> Self {
          Self {
              handed_out: AtomicU64::new(0),
          }
      }
  }

  impl IdSource for SequentialIds {
      fn session_id(&self) -> String {
          format!(
              "session-{}",
              self.handed_out.fetch_add(1, Ordering::Relaxed) + 1
          )
      }
  }
  ```

- [ ] Run the test and the whole workspace check; confirm green:

  ```
  cargo xtask check
  # expected: ends with "xtask check: ok"
  ```

- [ ] Commit: `feat(protocol): inject time and identifiers through two traits`

### Task 10: The plans say where the phase is

Files: modified `docs/plans/project-plan.md`, `CLAUDE.md`, `README.md`, `docs/plans/phase-2-protocol-store-cli/step-01-protocol-crate.md`

Consumes: nothing
Produces: a project plan, a `README.md`, and a `CLAUDE.md` that describe the repository as it now is

This task changes documentation and has no test cycle. As in Task 3, the `> ` marker on each block below is this plan's and is not part of the text to write.

- [ ] In `docs/plans/project-plan.md`, in the status paragraph, replace the sentences about phase 1 and phase 2 with:

  > Phase 1 is done and merged (pull request #5, 2026-09-16): all nine of its step plans under `docs/plans/phase-1-harness/` are executed and reviewed; three rules of section 5 that need a decision in the spec rather than a function in `core` are named at the end of that phase's section. Phase 2 is fully decided and under way; its step plans are written one at a time under `docs/plans/phase-2-protocol-store-cli/` as each step starts.

- [ ] In `docs/plans/project-plan.md`, in "Decisions that apply to every phase", append to the "Schemas own their types" bullet:

  > Generated types derive `PartialEq` (`TypeSpaceSettings::with_derive`), so that a test can compare a generated value to an expected one; this holds for every schema the workspace adds, not only the ones that have it today.

  Note, not text to write: the derive is turned on for the whole generator by phase 2 step 01, so a later step that adds a schema inherits it rather than rediscovering why it is there.

- [ ] In `CLAUDE.md`, replace the "Current state" section's paragraphs with:

  > Phase 0 (foundation) is done and merged (pull request #4): the Cargo workspace, `cargo xtask check`, the contract types generated from the schema, and `validate_contract` exist.
  >
  > Phase 1 (harness core) is done and merged (pull request #5): `farik-core` decides every rule of `docs/SPEC.md` section 5 that is a decision rather than an effect. Three rules of section 5 are recorded in `docs/plans/project-plan.md` as needing a decision in the spec rather than a function in `core`; read that note before adding one of them by hand.
  >
  > Phase 2 (protocol, store, and the first command line) is in progress on `claude/phase-0-implementation-izm38y` (the harness-assigned branch, reused because a session may not push to another branch without permission), in pull request #6. Its step plans live under `docs/plans/phase-2-protocol-store-cli/` and are written one at a time from `docs/plans/project-plan.md`.

- [ ] In `README.md`, replace the status line with:

  > Status: phase 0 (foundation) and phase 1 (harness core) merged; phase 2 (protocol, store, and the first command line) in progress. Nothing runs for a user yet. Project standards are in place; see [CONTRIBUTING.md](CONTRIBUTING.md) before making a change.

- [ ] Confirm the phase's draft pull request exists, is titled `phase 2: protocol, store, and the first command line`, follows `.github/pull_request_template.md`, and links this step plan. Open it if it does not; hard rule 11 of `CLAUDE.md` gives a pushed phase branch no other option. It is pull request #6, opened when this plan was pushed.

- [ ] Set this plan's `Status:` to `done` and confirm every checkbox above is ticked.

- [ ] Commit: `docs(docs): record phase 2 step 01 as done`

## Verification

The full check, run fresh on the final commit:

```
cargo xtask check
# expected: the format check, clippy with -D warnings, the workspace tests, the generated-file
#           freshness check for all four schemas, the bare-TODO check, and the core no-I/O check,
#           ending with "xtask check: ok"
```

The step's own behavior, run on its own:

```
cargo test -p farik-protocol
# expected: 29 tests passing -- 2 in lib, 17 in event::tests, 7 in command::tests, 3 in
#           clock::tests -- and none filtered out or ignored
```

The generated files are the schemas':

```
cargo xtask generate --check
# expected: no output and exit code 0
```

## Open questions

None.

