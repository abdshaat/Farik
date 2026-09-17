# Phase 2, step 03: Projections

Status: done
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; a session may not push to another branch without permission, so phase 2 reuses it as phase 1 did; steps do not get their own)
Spec: `docs/SPEC.md` section 8.4 (storage), 10 (the UI stays responsive with ten thousand events), 5.1 (every action is an event), 5.2 (the lifecycle the board shows), 5.11 (a locked contract), 5.16 (triage)
Depends on: phase 0 (merged in #4), phase 1 (merged in #5), steps 01 and 02 of this phase (committed as 1f93550 and 50b264e)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a fresh Claude Code review session, 2026-09-17, on the third round. It
rebuilt the step outside the working tree from this plan's own blocks, applied in task order with
nothing guessed, and reached `xtask check: ok` with every count this plan states: 225 `farik-core`,
37 `farik-protocol`, 30 the store's modules, 7 its integration test, 24 `xtask`. `git status
--porcelain` was nine paths identical to the File map with `Cargo.toml` and `Cargo.lock` untouched;
all six commit subjects were accepted by `cargo xtask commit-msg`; every format and lint step was
silent; Task 3's red produced its two behavioural assertion failures rather than a compile error; and
Task 4's mutation killed the reopen test with `left: 0`, `right: 1`.

The first reviewer rebuilt the step outside the working tree from this plan's own blocks and reached
`xtask check: ok` with every count exact, but three of the plan's own commands did not produce the
output it stated: Task 1's commit subject was 73 characters against a limit of 72, Task 2's
`contract.rs` block was not what rustfmt packs, and Task 2 imported `TaskId` a task before any test
names it, so its own `clippy -- -D warnings` step failed. Those are fixed, along with nine smaller
findings.

The twelfth was the one that mattered. Following the plan's instruction to watch Task 4's reopen test
fail by mutation showed that it did not fail: opening catches up from the log, so the board is
correct wherever it lives, and the test's assertions — made through `Projections` — could not tell a
persisted board from one rebuilt on the spot. The test now asks another connection to the log's file
what is in `task_projections` and `projection_cursor`, which is the property it was supposed to hold,
and the mutation kills it (`left: 0`, `right: 1`).

The second reviewer rebuilt it again and reached `xtask check: ok` with every count exact, the
changed-file set exactly the File map, all six subjects accepted, every format and lint step silent,
and Task 4's mutation producing the values this plan predicts. It refused on one finding, and that
one was made by the first round's fix: moving `TaskId` into Task 3 put it one checklist item too
late, so Task 3's red was a missing-import compile error rather than the two behavioural assertions
the plan states — the failure `docs/standards/workflow.md` stage 3 names as the one that does not
count. The import now goes in with the tests, before the red. Three smaller findings with it: the
placement of `TaskProjection` had a literal reading that left `Projections` undocumented, a Decisions
bullet disagreed with the text Task 6 writes about who first reads a cost, and the mutation step's
prose claimed more than the mutation does.

## Goal

The log answers "what happened"; after this step the store also answers "where is everything now" without reading the log to do it. `Projections` keeps one row per contract in the same database as the log — what kind it is, its parent, title, status and risk, whether triage has sized it and whether the human holds it — and moves that row on as each event arrives. A cursor records how far into the log the projections have read, so opening them costs nothing when they are already current, and a crash between an append and a projection is work left behind rather than damage: opening catches up. Nothing in the two tables is a source of truth, so `rebuild` throwing them away and reading the log again is always correct. `docs/SPEC.md` section 10 is what this is for: ten thousand events in a project, and a board that is one query.

## Decisions

- The projections live in the log's own database and share its connection and its lock: chose that over a second connection to the same file because a log opened at `:memory:` cannot be reached by a second connection at all, so the projections of a test log would be untestable; and because sharing the lock means a view cannot read a half-written append. `EventLog::connection` becomes `pub(crate)` for it.
- `Projections` holds an `Arc<EventLog>` and `open_projections` takes one: chose that over `open_projections(log: &EventLog)`, which the project plan records, because `rebuild` and the catch-up on open both have to read the log, and a `Projections` that borrows the log could not be held beside it — phase 3's `ToolContext` and `OrchestratorDeps` hold `Arc<EventLog>` and `Arc<Projections>` side by side. Keeping the `Arc` inside is what lets `rebuild(&self)` keep the signature the plan records. Task 6 records the change.
- `TaskProjection` carries only the fields an event of this phase can fill — `task_id`, `kind`, `parent`, `title`, `status`, `risk`, `triaged`, `locked`, `updated_seq`. The `assignee_id`, `reviewer_id`, `sprint_id` and `iteration` the project plan lists arrive with `task.transitioned` in phase 3 step 03, which is the event that carries them: a column nothing can write is a column no test can hold to anything, and the plan already defers `cost_usd` to 3.09 and three flags to 3.10 on exactly this reasoning. Task 6 records it.
- `CostScope`, `CostProjection` and `Projections::costs` are deferred to phase 3 step 09, where `cost.recorded` arrives: chose that over building them now because no event of this phase carries a cost, the first thing that reads one is phase 3 step 09's own `budget_state`, which arrives beside the event, and no view reads one until phase 5 step 04, so the only test possible would be one that wrote the table by hand and read it back — a test of SQLite rather than of Farik. Task 6 records it.
- An event at or before the cursor is ignored rather than applied again: chose idempotence over trusting the caller because a command that appends, projects, and also catches up on the next open would otherwise take one append in twice, and `updated_seq` would go backwards. The row and the cursor move in one transaction, so the cursor never claims work that was not done.
- Opening catches up; `rebuild` resets the cursor and replays: chose two operations over one because they answer different questions. Catching up is the ordinary path and costs nothing when there is nothing to catch up on. `rebuild` is the repair, for a row that drifted for any reason at all — a bug fixed since, a row changed by hand, a migration that added a column.
- An update that matches no row is a no-op, not a refusal: a `contract.locked` whose `task.created` is missing means a log that cannot be right, and a row needs a title, a status and a risk that only a summary carries. Refusing inside the store would make one bad row unread the whole board, which is the shape of defect the step 02 review found in `read`. `farik doctor` (step 05) is what reports a log and its files disagreeing, and it needs a board it can read to do that.
- `write_summary` upserts rather than inserts, for the same reason from the other side: a `contract.written` that arrives without its `task.created` still puts the contract on the board, because a summary carries everything a row needs.
- The board is ordered by the number in the task id, not by the id as text: `FRK-10` sorts before `FRK-9` as text, and a board that puts the tenth task before the ninth is a board nobody trusts. The offset the number starts at is derived from `TASK_ID_PREFIX` and bound as a parameter, so the prefix has one spelling.
- The three vocabularies an event repeats from the contract schema — kind, status, risk — are mapped to `farik-core`'s own types at the store's edge, which is the one mapping layer `docs/standards/code.md` allows per crate. The mappings are total because `farik-protocol` already has a test that fails when the two schemas drift; they are written out rather than derived so that a value added to one schema and not the other stops compiling here.
- `farik-core` gains a `TaskKind` alias for `FarikTaskContractKind`: it aliases every other contract vocabulary already, and the projection needs a name for this one. Task 6 records it.
- The cursor table holds one row, enforced by `CHECK (id = 1)`: two cursors would make "how far have the projections read" a question with two answers.
- `triaged` and `locked` are `INTEGER` columns with `CHECK (... IN (0, 1))`: SQLite has no boolean, and `STRICT` alone would accept 2.
- No new dependency, and `Cargo.lock` does not change.

## Design

`crates/store/src/projections.rs` holds `TaskProjection` (what a board shows about one contract), `Projections` (the derived tables of one log), and `open_projections`. Migration `0002_projections.sql` adds `task_projections`, two indexes on it, and `projection_cursor`.

Five of the nine event kinds are about one contract, and the protocol crate refuses one of those without a task id, so the projection always has a row to name. `task.created` and `contract.written` carry a summary and write every field from it. `request.triaged` sets the flag and decides the kind: `large` is an epic, `small` is a task (5.16 item 1). `contract.locked` and `contract.unlocked` set `locked`. The other four kinds — a scan, a team, a criterion library, a drift report — are about the project rather than a contract, and move the cursor without touching a row.

Out of scope: the cost projection and its scopes (phase 3 step 09); the assignee, reviewer, sprint and iteration fields (phase 3 step 03); anything that reads the board (step 06's `farik board` and `farik task show`); reconciliation between the board and the files (step 05). Nothing outside this crate's own tests reads a projection yet.

## Architecture notes

- Modified: `crates/store` gains `projections`, a sibling of `event_log` in the same crate and the same database. `EventLog::connection` and `TASK_ID_PREFIX` become `pub(crate)`; nothing else in `event_log.rs` changes.
- Modified: `crates/core/src/contract.rs` gains one alias, `TaskKind`.
- Consumed from `farik-protocol` (on this branch): `FarikEvent`, `EventBody`, `ContractSummary`, `ContractSummaryKind`, `ContractSummaryStatus`, `ContractSummaryRisk`, `RequestTriagedBodySize`, and `fixtures::{a_new_event, an_event_wire, a_contract_summary_wire}` for the tests.
- Consumed from `farik-core` (on `main`, plus the alias this step adds): `TaskId`, `TaskKind`, `TaskStatus`, `Risk`.
- Consumed from step 02 (on this branch): `EventLog`, `EventQuery`, `open_event_log`, `IN_MEMORY`, `StoreError`.
- `farik-core` does no I/O and is not touched beyond the alias; `cargo xtask core-io` still passes.

## Global constraints

- Wire and file formats are `snake_case`; every column spells its field exactly as the event schema does.
- Every value that reaches SQL is a bound parameter. The one thing a query's text varies is the offset the board is ordered by, which is bound too.
- No `unwrap` or `expect` outside tests. The lock is recovered from poisoning with `PoisonError::into_inner`, as `EventLog` does.
- Every table is `STRICT`, and a boolean column carries a `CHECK`.
- Nothing in the projection tables is a source of truth: every row is derivable from the log.
- The log's connection is behind one lock, and `std::sync::Mutex` is not reentrant: nothing may hold that guard across a call that takes it again. `catch_up` is why this is written down — it reads the cursor, reads the log, and applies each event as three separate acquisitions rather than one.
- No test is skipped, ignored, or quarantined.

## File map

```
crates/store/src/migrations/0002_projections.sql creates: task_projections, its indexes, projection_cursor
crates/store/src/projections.rs                creates: TaskProjection, Projections, open_projections; tested by its own tests module
crates/store/src/migrations.rs                 modifies: migration 0002 joins the list
crates/store/src/lib.rs                        modifies: the projections module and what it re-exports
crates/store/src/event_log.rs                  modifies: connection() and TASK_ID_PREFIX become pub(crate)
crates/core/src/contract.rs                    modifies: the TaskKind alias
crates/store/tests/event_log_file.rs           modifies: the board and its place survive a reopen
docs/plans/project-plan.md                     modifies: records what this step's interface became
docs/plans/phase-2-protocol-store-cli/step-03-projections.md modifies: this plan, ticked as it goes
```

`Cargo.toml` and `Cargo.lock` do not change: this step adds no dependency.

## Tasks

### Task 1: The projection tables, and how far they have read

Files: created `crates/store/src/migrations/0002_projections.sql`, `crates/store/src/projections.rs`; modified `crates/store/src/migrations.rs`, `crates/store/src/lib.rs`, `crates/store/src/event_log.rs`, `docs/plans/phase-2-protocol-store-cli/step-03-projections.md`; tested by `crates/store/src/projections.rs`

Consumes: `EventLog`, `open_event_log`, `IN_MEMORY`, `StoreError`, `migrations::known_versions` from step 02
Produces: `farik_store::{Projections, open_projections}`, `Projections::cursor`

- [x] Write the failing test. Create `crates/store/src/projections.rs` with the module doc:

  ```rust
  //! The board, read from the log rather than scanned out of it (`docs/SPEC.md` sections 8.4 and 10).
  ```

  then append the tests module:

  ```rust
  #[cfg(test)]
  mod tests {
      use chrono::{DateTime, TimeZone, Utc};

      use super::{Arc, EventLog, Projections, open_projections};
      use crate::event_log::{IN_MEMORY, open_event_log};
      use crate::migrations;

      fn at(hour: u32) -> DateTime<Utc> {
          Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
              .single()
              .expect("a real hour")
      }

      fn a_log() -> Arc<EventLog> {
          Arc::new(
              open_event_log(std::path::Path::new(IN_MEMORY), at(9)).expect("a log in memory opens"),
          )
      }

      /// A log and the projections of it, both empty.
      fn a_board() -> (Arc<EventLog>, Projections) {
          let log = a_log();
          let projections = open_projections(Arc::clone(&log)).expect("the projections open");
          (log, projections)
      }

      #[test]
      fn opens_the_projections_of_a_log_that_has_read_nothing_yet() {
          let (log, projections) = a_board();
          assert_eq!(projections.cursor().expect("the cursor reads"), 0);
          // The projection tables are part of the log's own database, applied as a migration like
          // everything else in it, so opening the log is what makes them.
          assert_eq!(
              log.applied_migrations().expect("the ledger reads"),
              migrations::known_versions()
          );
          assert_eq!(migrations::known_versions(), vec![1, 2]);
      }
  }
  ```

- [x] Declare the module and re-export it. In `crates/store/src/lib.rs`, replace the five lines from `/// The database's shape` to the `pub use event_log::` line, the blank line between them included, with:

  ```rust
  /// The database's shape, as SQL applied in order.
  pub mod migrations;
  /// The board, derived from the log.
  pub mod projections;

  pub use error::StoreError;
  pub use event_log::{EventLog, EventQuery, IN_MEMORY, open_event_log};
  pub use projections::{Projections, open_projections};
  ```

- [x] Run it and confirm it fails because there is nothing to open:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile, twice. Both errors appear when the target directory is warm,
  # which it is after the baseline check stage 3 asks for; from cold, cargo stops after the
  # first and prints `warning: build failed, waiting for other jobs to finish...`:
  # error[E0432]: unresolved imports `projections::Projections`, `projections::open_projections`
  # error[E0432]: unresolved imports `super::Arc`, `super::EventLog`, `super::Projections`,
  #   `super::open_projections`
  ```

- [x] Write the minimal implementation. Create `crates/store/src/migrations/0002_projections.sql`:

  ```sql
  -- The board, derived from the log (docs/SPEC.md 5.1, 8.4, and 10).
  --
  -- Nothing here is a source of truth: every row is derivable from the events, and dropping the two
  -- tables and replaying the log is always correct. They exist because `docs/SPEC.md` section 10 asks
  -- the UI to stay responsive with ten thousand events in a project, which a scan of the log per view
  -- is not, and because a view wants one row per contract rather than the history of one.

  -- One row per contract, holding what a board shows. The fields a phase 2 event carries and no
  -- others: an assignee, a reviewer, a sprint and an iteration arrive with `task.transitioned` in
  -- phase 3, and a column nothing can write is a column no test can hold to anything.
  CREATE TABLE task_projections (
      task_id     TEXT PRIMARY KEY,
      kind        TEXT NOT NULL,
      parent      TEXT,
      title       TEXT NOT NULL,
      status      TEXT NOT NULL,
      risk        TEXT NOT NULL,
      triaged     INTEGER NOT NULL CHECK (triaged IN (0, 1)),
      locked      INTEGER NOT NULL CHECK (locked IN (0, 1)),
      updated_seq INTEGER NOT NULL
  ) STRICT;

  CREATE INDEX task_projections_by_status ON task_projections (status, task_id);
  CREATE INDEX task_projections_by_parent ON task_projections (parent, task_id)
      WHERE parent IS NOT NULL;

  -- How far into the log the projections have read. One row, and the `CHECK` is what keeps it one: a
  -- second cursor would make "how far" a question with two answers.
  CREATE TABLE projection_cursor (
      id  INTEGER PRIMARY KEY CHECK (id = 1),
      seq INTEGER NOT NULL
  ) STRICT;
  ```

- [x] Add it to the list in `crates/store/src/migrations.rs`, replacing

  ```rust
  const MIGRATIONS: [Migration; 1] = [Migration {
      version: 1,
      sql: include_str!("migrations/0001_event_log.sql"),
  }];
  ```

  with:

  ```rust
  const MIGRATIONS: [Migration; 2] = [
      Migration {
          version: 1,
          sql: include_str!("migrations/0001_event_log.sql"),
      },
      Migration {
          version: 2,
          sql: include_str!("migrations/0002_projections.sql"),
      },
  ];
  ```

- [x] Let the projections reach the log's connection. In `crates/store/src/event_log.rs`, replace

  ```rust
      /// The connection, recovering from a lock another thread poisoned by panicking. A panic
      /// somewhere else says nothing about this database, and refusing every later append because of
      /// it would turn one bug into a stopped team.
      fn connection(&self) -> MutexGuard<'_, Connection> {
  ```

  with:

  ```rust
      /// The one connection this database has, recovering from a lock another thread poisoned by
      /// panicking. A panic somewhere else says nothing about this database, and refusing every later
      /// append because of it would turn one bug into a stopped team.
      ///
      /// The projections live in this database beside the log and take this same lock, so a view
      /// cannot read a half-written append, and a log opened in memory can be projected at all — a
      /// second connection could not reach it.
      pub(crate) fn connection(&self) -> MutexGuard<'_, Connection> {
  ```

- [x] Say in `crates/store/src/event_log.rs` why announcing under that lock is safe, now that something else takes it. Replace the line

  ```rust
          // Nothing takes the subscribers' lock before this one, so holding both cannot deadlock.
  ```

  with:

  ```rust
          // Nothing takes the subscribers' lock before this one, so holding both cannot deadlock.
          //
          // A subscriber's channel is unbounded, so this send cannot block, and the subscriber's own
          // work happens on its own thread after this returns. A bounded channel here would deadlock
          // against any subscriber that writes to this database — the projections do.
  ```

- [x] Insert into `crates/store/src/projections.rs`, between the module doc and the tests module:

  ```rust
  use std::sync::Arc;

  use rusqlite::Connection;

  use crate::error::StoreError;
  use crate::event_log::EventLog;
  ```

  then, after that:

  ```rust
  /// The projections of one log: derived tables that answer a view in one query.
  ///
  /// They share the log's connection and its lock, so a view cannot read a half-written append, and
  /// a log opened in memory can be projected at all.
  pub struct Projections {
      log: Arc<EventLog>,
  }
  ```

  then, after that:

  ```rust
  /// Opens the projections of `log`.
  ///
  /// # Errors
  ///
  /// `Sqlite` when a projection table cannot be read.
  pub fn open_projections(log: Arc<EventLog>) -> Result<Projections, StoreError> {
      Ok(Projections { log })
  }
  ```

  then, after that:

  ```rust
  impl Projections {
      /// How far into the log the projections have read: the sequence number of the last event
      /// applied, or zero when none has been.
      ///
      /// # Errors
      ///
      /// `Sqlite` when the cursor cannot be read.
      pub fn cursor(&self) -> Result<u64, StoreError> {
          let connection = self.connection();
          read_cursor(&connection)
      }

      /// The log's own connection: the projections live in the same database, and share its lock so
      /// that a view cannot read a half-written append.
      fn connection(&self) -> std::sync::MutexGuard<'_, Connection> {
          self.log.connection()
      }
  }
  ```

  and after that:

  ```rust
  fn read_cursor(connection: &Connection) -> Result<u64, StoreError> {
      let seq: i64 = connection
          .query_row(
              "SELECT seq FROM projection_cursor WHERE id = 1",
              (),
              |row| row.get(0),
          )
          .or_else(|error| match error {
              rusqlite::Error::QueryReturnedNoRows => Ok(0),
              other => Err(StoreError::from(other)),
          })?;
      u64::try_from(seq).map_err(|_| StoreError::Sqlite {
          detail: "the projection cursor is negative, which no event can produce".to_string(),
      })
  }
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 19 passed (the store's modules)
  #           test result: ok. 6 passed (event_log_file)
  ```

- [x] Run the format and lint checks:

  ```
  cargo fmt --all --check
  cargo clippy -p farik-store --all-targets -- -D warnings
  # expected: both silent
  ```

- [x] Commit: `feat(store): open a log's projections and say how far they have read`

### Task 2: A filed request reaches the board

Files: modified `crates/store/src/projections.rs`, `crates/store/src/lib.rs`, `crates/store/src/event_log.rs`, `crates/core/src/contract.rs`, `docs/plans/phase-2-protocol-store-cli/step-03-projections.md`; tested by `crates/store/src/projections.rs`

Consumes: `Projections`, `open_projections`, `Projections::cursor` from Task 1; `FarikEvent`, `EventBody`, `ContractSummary` and the three summary vocabularies from `crates/protocol/src/event.rs`; `TaskId`, `TaskStatus`, `Risk` from `crates/core/src/contract.rs`
Produces: `farik_store::TaskProjection`, `Projections::{apply, board, task}`, `farik_core::contract::TaskKind`

- [x] Write the failing tests. Replace the whole tests module of `crates/store/src/projections.rs` with:

  ```rust
  #[cfg(test)]
  mod tests {
      use chrono::{DateTime, TimeZone, Utc};
      use farik_protocol::event::fixtures::{a_contract_summary_wire, a_new_event, an_event_wire};
      use farik_protocol::event::{EventKind, NewEvent, event_from_value};
      use serde_json::json;

      use super::{Arc, EventLog, FarikEvent, Projections, TaskProjection, open_projections};
      use crate::error::StoreError;
      use crate::event_log::{IN_MEMORY, open_event_log};
      use farik_core::contract::{Risk, TaskKind, TaskStatus};

      fn at(hour: u32) -> DateTime<Utc> {
          Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
              .single()
              .expect("a real hour")
      }

      fn a_log() -> Arc<EventLog> {
          Arc::new(
              open_event_log(std::path::Path::new(IN_MEMORY), at(9)).expect("a log in memory opens"),
          )
      }

      /// A log and the projections of it, both empty.
      fn a_board() -> (Arc<EventLog>, Projections) {
          let log = a_log();
          let projections = open_projections(Arc::clone(&log)).expect("the projections open");
          (log, projections)
      }

      /// Appends one event and hands it to the projections, the way a command does.
      fn record(log: &EventLog, projections: &Projections, event: &NewEvent) -> FarikEvent {
          let appended = log.append(event).expect("appends");
          projections.apply(&appended).expect("projects");
          appended
      }

      /// The fixture event of one kind, about the contract `task_id`.
      fn about(kind: EventKind, task_id: &str) -> NewEvent {
          let mut event = a_new_event(kind);
          event.task_id = Some(task_id.parse().expect("a task id"));
          event
      }

      /// A `contract.written` whose summary says what the arguments say.
      fn written(
          task_id: &str,
          title: &str,
          status: &str,
          risk: &str,
          parent: Option<&str>,
      ) -> NewEvent {
          let mut summary = a_contract_summary_wire();
          summary["title"] = json!(title);
          summary["status"] = json!(status);
          summary["risk"] = json!(risk);
          if let Some(parent) = parent {
              summary["parent"] = json!(parent);
          }
          let mut wire = an_event_wire(EventKind::ContractWritten);
          wire["task_id"] = json!(task_id);
          wire["body"]["summary"] = summary;
          let event = event_from_value(&wire).expect("the fixture is schema-valid");
          NewEvent {
              recorded_at: event.envelope.recorded_at,
              team_id: event.envelope.team_id,
              project_id: event.envelope.project_id,
              task_id: event.envelope.task_id,
              agent_id: event.envelope.agent_id,
              session_id: event.envelope.session_id,
              body: event.body,
          }
      }

      fn ids_of(board: &[TaskProjection]) -> Vec<String> {
          board.iter().map(|task| task.task_id.to_string()).collect()
      }

      #[test]
      fn shows_a_request_on_the_board_as_soon_as_it_is_filed() {
          let (log, projections) = a_board();
          let filed = record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
          let board = projections.board().expect("the board reads");
          assert_eq!(
              board,
              vec![TaskProjection {
                  task_id: "FRK-1".parse().expect("a task id"),
                  kind: TaskKind::Task,
                  parent: None,
                  title: "Add a login page".to_string(),
                  status: TaskStatus::Draft,
                  risk: Risk::Low,
                  triaged: false,
                  locked: false,
                  updated_seq: filed.envelope.seq,
              }]
          );
          assert_eq!(projections.cursor().expect("the cursor reads"), 1);
      }

      #[test]
      fn takes_every_field_of_the_latest_contract_written() {
          let (log, projections) = a_board();
          record(&log, &projections, &about(EventKind::TaskCreated, "FRK-2"));
          let rewritten = record(
              &log,
              &projections,
              &written(
                  "FRK-2",
                  "Add a logout page",
                  "refining",
                  "high",
                  Some("FRK-1"),
              ),
          );
          assert_eq!(
              projections
                  .task(&"FRK-2".parse().expect("a task id"))
                  .expect("the read works")
                  .expect("on the board"),
              TaskProjection {
                  task_id: "FRK-2".parse().expect("a task id"),
                  kind: TaskKind::Task,
                  parent: Some("FRK-1".parse().expect("a task id")),
                  title: "Add a logout page".to_string(),
                  status: TaskStatus::Refining,
                  risk: Risk::High,
                  triaged: false,
                  locked: false,
                  updated_seq: rewritten.envelope.seq,
              }
          );
      }

      #[test]
      fn moves_the_cursor_past_an_event_that_is_about_no_contract() {
          // A scan, a team, a criterion library and a drift report change no row. The cursor still has
          // to pass them, or catching up would read them again on every open, for ever.
          let (log, projections) = a_board();
          for kind in [
              EventKind::ProjectScanned,
              EventKind::TeamUpdated,
              EventKind::CriteriaUpdated,
              EventKind::DriftDetected,
          ] {
              record(&log, &projections, &a_new_event(kind));
          }
          assert_eq!(projections.board().expect("the board reads"), Vec::new());
          assert_eq!(projections.cursor().expect("the cursor reads"), 4);
      }

      #[test]
      fn orders_the_board_by_the_number_in_the_id_rather_than_by_its_text() {
          // `FRK-10` sorts before `FRK-9` as text, and a board that puts the tenth task before the
          // ninth is a board nobody trusts.
          let (log, projections) = a_board();
          for id in ["FRK-2", "FRK-10", "FRK-1"] {
              record(&log, &projections, &about(EventKind::TaskCreated, id));
          }
          assert_eq!(
              ids_of(&projections.board().expect("the board reads")),
              ["FRK-1", "FRK-2", "FRK-10"]
          );
      }

      #[test]
      fn says_nothing_about_a_contract_it_never_saw_an_event_for() {
          let (log, projections) = a_board();
          record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
          assert_eq!(
              projections
                  .task(&"FRK-9".parse().expect("a task id"))
                  .expect("the read works"),
              None
          );
      }

      #[test]
      fn refuses_a_projected_row_it_cannot_read_back() {
          // Nothing this crate writes can produce such a row, so this is about the file having been
          // changed by something else, or written by a Farik this one does not understand.
          let (log, projections) = a_board();
          record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
          log.connection()
              .execute(
                  "UPDATE task_projections SET status = 'nearly done' WHERE task_id = 'FRK-1'",
                  (),
              )
              .expect("a row is changed by hand");
          let refusal = projections.board().expect_err("a status nothing spells");
          assert!(
              matches!(&refusal, StoreError::InvalidEvent { detail }
                  if detail.contains("FRK-1") && detail.contains("nearly done")),
              "{refusal:?}"
          );
      }
  }
  ```

- [x] Run them and confirm they fail because nothing is projected:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile,
  # these three codes and no others, nine errors in all, each E0599 once per call site:
  # error[E0432]: unresolved imports `super::FarikEvent`, `super::TaskProjection`
  # error[E0432]: unresolved import `farik_core::contract::TaskKind`
  # error[E0599]: no method named `apply` found for reference `&Projections` in the current scope
  # error[E0599]: no method named `board` found for struct `Projections` in the current scope
  # error[E0599]: no method named `task` found for struct `Projections` in the current scope
  ```

- [x] Give the contract's kind a name. In `crates/core/src/contract.rs`, replace the four lines of the `pub use` list from `FarikTaskContract as TaskContract` to `Role,`

  ```rust
      FarikTaskContract as TaskContract, FarikTaskContractBudget as Budget,
      FarikTaskContractId as TaskId, FarikTaskContractNotes as Notes,
      FarikTaskContractRequirementsItem as Requirement, FarikTaskContractRisk as Risk,
      FarikTaskContractStatus as TaskStatus, Role,
  ```

  with:

  ```rust
      FarikTaskContract as TaskContract, FarikTaskContractBudget as Budget,
      FarikTaskContractId as TaskId, FarikTaskContractKind as TaskKind,
      FarikTaskContractNotes as Notes, FarikTaskContractRequirementsItem as Requirement,
      FarikTaskContractRisk as Risk, FarikTaskContractStatus as TaskStatus, Role,
  ```

- [x] Let the board read the task id prefix. In `crates/store/src/event_log.rs`, replace

  ```rust
  /// The prefix every task id this store hands out carries, from the contract schema's pattern.
  const TASK_ID_PREFIX: &str = "FRK";
  ```

  with:

  ```rust
  /// The prefix every task id this store hands out carries, from the contract schema's pattern.
  pub(crate) const TASK_ID_PREFIX: &str = "FRK";
  ```

- [x] Write the minimal implementation. In `crates/store/src/projections.rs`, replace the import block — the six lines from `use std::sync::Arc;` to `use crate::event_log::EventLog;` — with:

  ```rust
  use std::str::FromStr;
  use std::sync::Arc;

  use farik_core::contract::{Risk, TaskId, TaskKind, TaskStatus};
  use farik_protocol::event::{
      ContractSummary, ContractSummaryKind, ContractSummaryRisk, ContractSummaryStatus, EventBody,
      FarikEvent,
  };
  use rusqlite::{Connection, Transaction};

  use crate::error::StoreError;
  use crate::event_log::{EventLog, TASK_ID_PREFIX};
  ```

  insert before the doc comment of `pub struct Projections` — above it, not between it and the struct, which would leave `Projections` undocumented and `missing_docs` refuses that:

  ```rust
  /// What a board shows about one contract, as the log left it.
  ///
  /// Every field comes from an event a phase 2 command emits. The assignee, the reviewer, the sprint
  /// and the iteration arrive with `task.transitioned` in phase 3, and `cost_usd` with
  /// `cost.recorded`; a column nothing can write is a column no test can hold to anything.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct TaskProjection {
      /// The contract this is about.
      pub task_id: TaskId,
      /// Whether it is an epic or a task, as the last event to say so said.
      pub kind: TaskKind,
      /// The epic this is under, when it is under one.
      pub parent: Option<TaskId>,
      /// The contract's title.
      pub title: String,
      /// Where the contract is in the lifecycle of `docs/SPEC.md` 5.2.
      pub status: TaskStatus,
      /// The contract's risk.
      pub risk: Risk,
      /// Whether triage has sized the request (5.16).
      pub triaged: bool,
      /// Whether the human holds the contract (5.11).
      pub locked: bool,
      /// The sequence number of the last event that changed this row, which is what says how fresh it
      /// is and which event to blame for what it says.
      pub updated_seq: u64,
  }
  ```

  insert into `impl Projections`, before `cursor`:

  ```rust
      /// Applies one event, and moves the cursor to it.
      ///
      /// The row and the cursor move in one transaction, so the cursor never claims work that was not
      /// done.
      ///
      /// # Errors
      ///
      /// `Sqlite` when the write fails.
      pub fn apply(&self, event: &FarikEvent) -> Result<(), StoreError> {
          let mut connection = self.connection();
          let transaction = connection.transaction()?;
          apply_to(&transaction, event)?;
          write_cursor(&transaction, event.envelope.seq)?;
          transaction.commit()?;
          Ok(())
      }
  ```

  then, still before `cursor`:

  ```rust
      /// Every contract the log knows about, oldest task id first.
      ///
      /// # Errors
      ///
      /// `Sqlite` when the read fails; `InvalidEvent` when a projected row cannot be read back.
      pub fn board(&self) -> Result<Vec<TaskProjection>, StoreError> {
          let connection = self.connection();
          let mut statement =
              connection.prepare(&format!("{SELECT_PROJECTION} ORDER BY {BY_NUMBER}"))?;
          let rows = statement.query_map((number_offset(),), projected_row)?;
          let mut board = Vec::new();
          for row in rows {
              board.push(projection_of_row(row?)?);
          }
          Ok(board)
      }
  ```

  then, still before `cursor`:

  ```rust
      /// One contract, or nothing when the log has no event about it.
      ///
      /// # Errors
      ///
      /// `Sqlite` when the read fails; `InvalidEvent` when the projected row cannot be read back.
      pub fn task(&self, task_id: &TaskId) -> Result<Option<TaskProjection>, StoreError> {
          let connection = self.connection();
          let mut statement =
              connection.prepare(&format!("{SELECT_PROJECTION} WHERE task_id = ?1"))?;
          let mut rows = statement.query_map((task_id.to_string(),), projected_row)?;
          match rows.next() {
              None => Ok(None),
              Some(row) => Ok(Some(projection_of_row(row?)?)),
          }
      }
  ```

  and append, after `read_cursor`:

  ```rust
  const SELECT_PROJECTION: &str = "SELECT task_id, kind, parent, title, status, risk, triaged, \
                                   locked, updated_seq FROM task_projections";

  /// The board is ordered by the number in the task id, not by the id itself: `FRK-10` sorts before
  /// `FRK-9` as text, and a board that puts the tenth task before the ninth is a board nobody trusts.
  const BY_NUMBER: &str = "CAST(substr(task_id, ?1) AS INTEGER)";

  /// Where the number starts in a task id, counting from one as `substr` does: past the prefix and
  /// the dash that follows it. Taken from the prefix rather than written down again.
  fn number_offset() -> i64 {
      i64::try_from(TASK_ID_PREFIX.len() + 2).unwrap_or(i64::MAX)
  }
  ```

  ```rust
  type ProjectedRow = (
      String,
      String,
      Option<String>,
      String,
      String,
      String,
      bool,
      bool,
      i64,
  );

  fn projected_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectedRow> {
      Ok((
          row.get(0)?,
          row.get(1)?,
          row.get(2)?,
          row.get(3)?,
          row.get(4)?,
          row.get(5)?,
          row.get(6)?,
          row.get(7)?,
          row.get(8)?,
      ))
  }

  fn projection_of_row(row: ProjectedRow) -> Result<TaskProjection, StoreError> {
      let (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq) = row;
      let refuse = |what: &str, value: &str| StoreError::InvalidEvent {
          detail: format!("the projection of {task_id} holds {value:?} as its {what}"),
      };
      Ok(TaskProjection {
          task_id: TaskId::from_str(&task_id).map_err(|_| refuse("id", &task_id))?,
          kind: TaskKind::from_str(&kind).map_err(|_| refuse("kind", &kind))?,
          parent: match parent {
              None => None,
              Some(parent) => Some(TaskId::from_str(&parent).map_err(|_| refuse("parent", &parent))?),
          },
          title,
          status: TaskStatus::from_str(&status).map_err(|_| refuse("status", &status))?,
          risk: Risk::from_str(&risk).map_err(|_| refuse("risk", &risk))?,
          triaged,
          locked,
          updated_seq: u64::try_from(updated_seq)
              .map_err(|_| refuse("sequence number", &updated_seq.to_string()))?,
      })
  }
  ```

  ```rust
  /// Applies one event to the projection tables, leaving the cursor to the caller.
  fn apply_to(transaction: &Transaction<'_>, event: &FarikEvent) -> Result<(), StoreError> {
      let Some(task_id) = &event.envelope.task_id else {
          // Only the five kinds that are about one contract touch the board, and the protocol crate
          // refuses one of those without a task id. The rest — a scan, a team, a criterion library,
          // a drift report — are about the project.
          return Ok(());
      };
      let seq = i64::try_from(event.envelope.seq).map_err(|_| StoreError::Sqlite {
          detail: format!(
              "event {} is past what the engine can hold",
              event.envelope.seq
          ),
      })?;
      let id = task_id.to_string();
      match &event.body {
          EventBody::TaskCreated(body) => write_summary(transaction, &id, &body.summary, seq),
          EventBody::ContractWritten(body) => write_summary(transaction, &id, &body.summary, seq),
          EventBody::RequestTriaged(_)
          | EventBody::ContractLocked(_)
          | EventBody::ContractUnlocked(_)
          | EventBody::DriftDetected(_)
          | EventBody::ProjectScanned(_)
          | EventBody::TeamUpdated(_)
          | EventBody::CriteriaUpdated(_) => Ok(()),
      }
  }
  ```

  ```rust
  /// Writes what a summary says, creating the row when this is the first event about the contract.
  ///
  /// An upsert rather than an insert, and an upsert rather than a refusal: a `contract.written` whose
  /// `task.created` is missing means a log that cannot be right, and refusing it here would make the
  /// whole board unreadable over one row. `farik doctor` is what reports a log and its files
  /// disagreeing (5.1), and it needs a board it can read to do that.
  fn write_summary(
      transaction: &Transaction<'_>,
      task_id: &str,
      summary: &ContractSummary,
      seq: i64,
  ) -> Result<(), StoreError> {
      transaction.execute(
          "INSERT INTO task_projections
               (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, ?7)
           ON CONFLICT (task_id) DO UPDATE SET
               kind = ?2, parent = ?3, title = ?4, status = ?5, risk = ?6, updated_seq = ?7",
          (
              task_id,
              kind_of(summary.kind).to_string(),
              summary.parent.as_ref().map(|parent| parent.to_string()),
              &summary.title,
              status_of(summary.status).to_string(),
              risk_of(summary.risk).to_string(),
              seq,
          ),
      )?;
      Ok(())
  }
  ```

  ```rust
  fn write_cursor(transaction: &Transaction<'_>, seq: u64) -> Result<(), StoreError> {
      let seq = i64::try_from(seq).map_err(|_| StoreError::Sqlite {
          detail: format!("event {seq} is past what the engine can hold"),
      })?;
      transaction.execute(
          "INSERT INTO projection_cursor (id, seq) VALUES (1, ?1)
           ON CONFLICT (id) DO UPDATE SET seq = ?1",
          (seq,),
      )?;
      Ok(())
  }
  ```

  ```rust
  /// The three vocabularies an event repeats from the contract schema, in the contract's own types.
  ///
  /// The two spellings are generated from two schemas, and a test in `farik-protocol` fails when they
  /// drift, so these mappings are total and stay total.
  fn kind_of(kind: ContractSummaryKind) -> TaskKind {
      match kind {
          ContractSummaryKind::Epic => TaskKind::Epic,
          ContractSummaryKind::Task => TaskKind::Task,
      }
  }

  fn status_of(status: ContractSummaryStatus) -> TaskStatus {
      match status {
          ContractSummaryStatus::Draft => TaskStatus::Draft,
          ContractSummaryStatus::Refining => TaskStatus::Refining,
          ContractSummaryStatus::Ready => TaskStatus::Ready,
          ContractSummaryStatus::Assigned => TaskStatus::Assigned,
          ContractSummaryStatus::InProgress => TaskStatus::InProgress,
          ContractSummaryStatus::Blocked => TaskStatus::Blocked,
          ContractSummaryStatus::Verifying => TaskStatus::Verifying,
          ContractSummaryStatus::Rejected => TaskStatus::Rejected,
          ContractSummaryStatus::Accepted => TaskStatus::Accepted,
          ContractSummaryStatus::Escalated => TaskStatus::Escalated,
          ContractSummaryStatus::Cancelled => TaskStatus::Cancelled,
      }
  }

  fn risk_of(risk: ContractSummaryRisk) -> Risk {
      match risk {
          ContractSummaryRisk::Low => Risk::Low,
          ContractSummaryRisk::Medium => Risk::Medium,
          ContractSummaryRisk::High => Risk::High,
      }
  }
  ```

- [x] Add `TaskProjection` to the re-export in `crates/store/src/lib.rs`, which becomes:

  ```rust
  pub use projections::{Projections, TaskProjection, open_projections};
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 24 passed (the store's modules)
  #           test result: ok. 6 passed (event_log_file)
  ```

- [x] Run the format and lint checks, across the workspace this time because `farik-core` changed:

  ```
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  # expected: both silent
  ```

- [x] Commit: `feat(store): put a filed request on the board`

### Task 3: Triage, and a contract the human holds

Files: modified `crates/store/src/projections.rs`, `docs/plans/phase-2-protocol-store-cli/step-03-projections.md`; tested by `crates/store/src/projections.rs`

Consumes: `Projections::{apply, board, task}` from Task 2
Produces: the `request.triaged`, `contract.locked` and `contract.unlocked` arms of `apply`

- [x] Write the failing tests. Append these three to the tests module of `crates/store/src/projections.rs`, a blank line between each and the test above it:

  ```rust
      #[test]
      fn takes_the_kind_and_the_flag_from_the_triage() {
          // Triage decides whether a request is an epic or a task, and the board is where a user sees
          // that it has happened at all (`docs/SPEC.md` 5.16 item 1).
          let (log, projections) = a_board();
          record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
          let triaged = record(
              &log,
              &projections,
              &about(EventKind::RequestTriaged, "FRK-1"),
          );
          let task = projections
              .task(&"FRK-1".parse().expect("a task id"))
              .expect("the read works")
              .expect("the contract is on the board");
          assert!(task.triaged);
          assert_eq!(task.kind, TaskKind::Task, "small is a task");
          assert_eq!(task.updated_seq, triaged.envelope.seq);

          let (log, projections) = a_board();
          record(&log, &projections, &about(EventKind::TaskCreated, "FRK-2"));
          let mut large = about(EventKind::RequestTriaged, "FRK-2");
          large.body = event_from_value(&{
              let mut wire = an_event_wire(EventKind::RequestTriaged);
              wire["task_id"] = json!("FRK-2");
              wire["body"]["size"] = json!("large");
              wire
          })
          .expect("the fixture is schema-valid")
          .body;
          record(&log, &projections, &large);
          assert_eq!(
              projections
                  .task(&"FRK-2".parse().expect("a task id"))
                  .expect("the read works")
                  .expect("on the board")
                  .kind,
              TaskKind::Epic,
              "large is an epic"
          );
      }

      #[test]
      fn says_who_holds_a_contract_the_human_locked_and_gave_back() {
          let (log, projections) = a_board();
          record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
          let id: TaskId = "FRK-1".parse().expect("a task id");
          let held = |projections: &Projections| {
              projections
                  .task(&id)
                  .expect("the read works")
                  .expect("on the board")
                  .locked
          };
          record(
              &log,
              &projections,
              &about(EventKind::ContractLocked, "FRK-1"),
          );
          assert!(held(&projections), "locked");
          record(
              &log,
              &projections,
              &about(EventKind::ContractUnlocked, "FRK-1"),
          );
          assert!(!held(&projections), "given back");
      }

      #[test]
      fn has_nothing_to_say_about_a_contract_whose_first_event_is_missing() {
          // A lock names a contract some earlier event created, and a row needs a title, a status and
          // a risk that only a summary carries. Refusing here would make one row unread the whole
          // board; reconciliation is what reports a log that cannot be right.
          let (log, projections) = a_board();
          record(
              &log,
              &projections,
              &about(EventKind::ContractLocked, "FRK-4"),
          );
          assert_eq!(projections.board().expect("the board reads"), Vec::new());
          assert_eq!(projections.cursor().expect("the cursor reads"), 1);
      }
  ```

- [x] Add `TaskId` to the tests module's `farik_core::contract` import, which these tests are the first to name, so that it becomes:

  ```rust
  use farik_core::contract::{Risk, TaskId, TaskKind, TaskStatus};
  ```

- [x] Run them and confirm they fail because a triage and a lock change nothing:

  ```
  cargo test -p farik-store --lib
  # expected: FAIL
  # ---- projections::tests::takes_the_kind_and_the_flag_from_the_triage stdout ----
  # assertion failed: task.triaged
  # ---- projections::tests::says_who_holds_a_contract_the_human_locked_and_gave_back stdout ----
  # locked
  # test result: FAILED. 25 passed; 2 failed
  ```

- [x] Write the minimal implementation. In `crates/store/src/projections.rs`, add `RequestTriagedBodySize` to the `farik_protocol::event` import, which becomes:

  ```rust
  use farik_protocol::event::{
      ContractSummary, ContractSummaryKind, ContractSummaryRisk, ContractSummaryStatus, EventBody,
      FarikEvent, RequestTriagedBodySize,
  };
  ```

  replace the first four arms of the catch-all in `apply_to` —

  ```rust
          EventBody::RequestTriaged(_)
          | EventBody::ContractLocked(_)
          | EventBody::ContractUnlocked(_)
          | EventBody::DriftDetected(_)
  ```

  — with:

  ```rust
          EventBody::RequestTriaged(body) => {
              // Triage decides the kind as well as recording that it happened (5.16 item 1).
              let kind = match body.size {
                  RequestTriagedBodySize::Large => TaskKind::Epic,
                  RequestTriagedBodySize::Small => TaskKind::Task,
              };
              update(
                  transaction,
                  "UPDATE task_projections SET triaged = 1, kind = ?2, updated_seq = ?3
                   WHERE task_id = ?1",
                  (&id, kind.to_string(), seq),
              )
          }
          EventBody::ContractLocked(_) => set_locked(transaction, &id, true, seq),
          EventBody::ContractUnlocked(_) => set_locked(transaction, &id, false, seq),
          EventBody::DriftDetected(_)
  ```

  and insert before `read_cursor`:

  ```rust
  fn set_locked(
      transaction: &Transaction<'_>,
      task_id: &str,
      locked: bool,
      seq: i64,
  ) -> Result<(), StoreError> {
      update(
          transaction,
          "UPDATE task_projections SET locked = ?2, updated_seq = ?3 WHERE task_id = ?1",
          (task_id, locked, seq),
      )
  }

  /// Runs an update that has nothing to say about a contract the board has never heard of.
  ///
  /// A lock, an unlock or a triage names a contract some earlier event created. When no row matches,
  /// the log is missing that earlier event, and there is nothing this table can invent: a row needs a
  /// title, a status and a risk, and only a summary carries them. Reconciliation is what reports it.
  fn update(
      transaction: &Transaction<'_>,
      sql: &str,
      parameters: impl rusqlite::Params,
  ) -> Result<(), StoreError> {
      transaction.execute(sql, parameters)?;
      Ok(())
  }
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 27 passed (the store's modules)
  #           test result: ok. 6 passed (event_log_file)
  ```

- [x] Run the format and lint checks:

  ```
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  # expected: both silent
  ```

- [x] Commit: `feat(store): record a triage and a contract the human holds`

### Task 4: Catching up, and taking one event once

Files: modified `crates/store/src/projections.rs`, `crates/store/tests/event_log_file.rs`, `docs/plans/phase-2-protocol-store-cli/step-03-projections.md`; tested by both

Consumes: `Projections::{apply, board, cursor}` from Tasks 1 to 3; `EventQuery` from step 02
Produces: the catch-up in `open_projections`, and an `apply` that takes one event once

- [x] Write the failing tests. Append these two to the tests module of `crates/store/src/projections.rs`, a blank line between each and the test above it:

  ```rust
      #[test]
      fn catches_up_with_everything_the_log_holds_when_it_is_opened() {
          // The projections are derived, so a process that appended and stopped before projecting has
          // left work behind rather than damage. Opening is where it is done.
          let log = a_log();
          for id in ["FRK-1", "FRK-2"] {
              log.append(&about(EventKind::TaskCreated, id))
                  .expect("appends");
          }
          let projections = open_projections(Arc::clone(&log)).expect("the projections open");
          assert_eq!(
              ids_of(&projections.board().expect("the board reads")),
              ["FRK-1", "FRK-2"]
          );
          assert_eq!(projections.cursor().expect("the cursor reads"), 2);
          // And opening again reads nothing twice.
          let again = open_projections(Arc::clone(&log)).expect("the projections open again");
          assert_eq!(again.cursor().expect("the cursor reads"), 2);
          assert_eq!(again.board().expect("the board reads").len(), 2);
      }

      #[test]
      fn applies_one_event_once_however_often_it_is_handed_over() {
          // A caller that both subscribes and catches up on open hands the same append over twice.
          let (log, projections) = a_board();
          let filed = record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
          let rewritten = record(
              &log,
              &projections,
              &written("FRK-1", "Add a logout page", "refining", "low", None),
          );
          projections.apply(&filed).expect("the first one again");
          let task = projections
              .task(&"FRK-1".parse().expect("a task id"))
              .expect("the read works")
              .expect("on the board");
          assert_eq!(
              task.title, "Add a logout page",
              "the later event still stands"
          );
          assert_eq!(task.updated_seq, rewritten.envelope.seq);
          assert_eq!(projections.cursor().expect("the cursor reads"), 2);
      }
  ```

- [x] Append to `crates/store/tests/event_log_file.rs`:

  ```rust
  #[test]
  fn keeps_the_board_and_its_place_in_the_log_across_a_reopen() {
      // The projections are derived, but they are derived once: `docs/SPEC.md` section 10 asks that a
      // command opening a project not replay ten thousand events to show a board. That is only true if
      // the board is in the log's own file, which is what another connection to that file can say.
      let directory = TempDir::new("board-across-a-reopen");
      {
          let log =
              std::sync::Arc::new(open_event_log(&directory.db(), at(9)).expect("the log opens"));
          let projections =
              farik_store::open_projections(std::sync::Arc::clone(&log)).expect("projections open");
          for kind in [EventKind::TaskCreated, EventKind::RequestTriaged] {
              let appended = log.append(&an_event(kind)).expect("appends");
              projections.apply(&appended).expect("projects");
          }
      }
      let outside = rusqlite::Connection::open(directory.db()).expect("another connection");
      let projected: i64 = outside
          .query_row("SELECT count(*) FROM task_projections", [], |row| {
              row.get(0)
          })
          .expect("the board reads");
      assert_eq!(projected, 1, "the board is in the log's own file");
      let cursor: i64 = outside
          .query_row(
              "SELECT seq FROM projection_cursor WHERE id = 1",
              [],
              |row| row.get(0),
          )
          .expect("the cursor reads");
      assert_eq!(cursor, 2, "and so is how far it had read");
      drop(outside);

      // So a command that opens the project again has nothing to catch up on, and reads the board the
      // events left rather than one it built itself.
      let log =
          std::sync::Arc::new(open_event_log(&directory.db(), at(10)).expect("the log reopens"));
      let projections =
          farik_store::open_projections(std::sync::Arc::clone(&log)).expect("projections reopen");
      assert_eq!(projections.cursor().expect("the cursor reads"), 2);
      let board = projections.board().expect("the board reads");
      assert_eq!(board.len(), 1, "both events are about the one contract");
      assert!(board[0].triaged, "and the triage is still recorded");
  }
  ```

- [x] Run them and confirm the first two fail because opening reads nothing and an event is taken twice:

  ```
  cargo test -p farik-store --lib
  # expected: FAIL
  # ---- projections::tests::applies_one_event_once_however_often_it_is_handed_over stdout ----
  # assertion `left == right` failed: the later event still stands
  #   left: "Add a login page"
  #  right: "Add a logout page"
  # ---- projections::tests::catches_up_with_everything_the_log_holds_when_it_is_opened stdout ----
  # assertion `left == right` failed
  # test result: FAILED. 27 passed; 2 failed
  ```

  The reopen test in `crates/store/tests/event_log_file.rs` passes on arrival, and no missing
  feature can make it fail: it holds a property of *where* the projections live rather than of
  what they do. It is watched to fail after the green instead, by mutation — the step below.

- [x] Write the minimal implementation. In `crates/store/src/projections.rs`, add `EventQuery` to the `crate::event_log` import, which becomes:

  ```rust
  use crate::event_log::{EventLog, EventQuery, TASK_ID_PREFIX};
  ```

  replace `open_projections` with:

  ```rust
  /// Opens the projections of `log` and brings them up to date with it.
  ///
  /// Catching up on open is what makes the pair self-healing: an `apply` that never ran, because the
  /// process stopped between the append and the projection, is applied here instead. The cursor is
  /// what remembers how far the projections had read.
  ///
  /// # Errors
  ///
  /// `Sqlite` when a projection table cannot be read or written; `InvalidEvent` when the log holds a
  /// row that is not an event.
  pub fn open_projections(log: Arc<EventLog>) -> Result<Projections, StoreError> {
      let projections = Projections { log };
      projections.catch_up()?;
      Ok(projections)
  }
  ```

  replace `apply` with:

  ```rust
      /// Applies one event, and moves the cursor to it.
      ///
      /// An event at or before the cursor is ignored rather than applied twice: a caller that both
      /// subscribes and catches up on open would otherwise take the same append in twice, and
      /// `updated_seq` would go backwards. The row and the cursor move in one transaction, so the
      /// cursor never claims work that was not done.
      ///
      /// # Errors
      ///
      /// `Sqlite` when the write fails.
      pub fn apply(&self, event: &FarikEvent) -> Result<(), StoreError> {
          let mut connection = self.connection();
          let transaction = connection.transaction()?;
          if event.envelope.seq <= read_cursor(&transaction)? {
              return Ok(());
          }
          apply_to(&transaction, event)?;
          write_cursor(&transaction, event.envelope.seq)?;
          transaction.commit()?;
          Ok(())
      }
  ```

  and insert into `impl Projections`, before `connection`:

  ```rust
      /// Applies every event the log has that the cursor has not reached.
      fn catch_up(&self) -> Result<(), StoreError> {
          let after_seq = self.cursor()?;
          let behind = self.log.read(&EventQuery {
              after_seq: Some(after_seq),
              ..EventQuery::default()
          })?;
          for event in &behind {
              self.apply(event)?;
          }
          Ok(())
      }
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 29 passed (the store's modules)
  #           test result: ok. 7 passed (event_log_file)
  ```

- [x] Run the format and lint checks:

  ```
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  # expected: both silent
  ```

- [x] Confirm the reopen test is a pin rather than a passenger. Make these three edits, which give `Projections` a database of its own instead of the log's:

  ```rust
  // in the struct:
  pub struct Projections {
      log: Arc<EventLog>,
      own: std::sync::Mutex<Connection>,
  }

  // in open_projections, in place of the line `let projections = Projections { log };`:
      let mut own = Connection::open_in_memory()?;
      crate::migrations::apply(&mut own, chrono::Utc::now())?;
      let projections = Projections {
          log,
          own: std::sync::Mutex::new(own),
      };

  // in connection, in place of the line `self.log.connection()`:
          self.own
              .lock()
              .unwrap_or_else(std::sync::PoisonError::into_inner)
  ```

  and run it:

  ```
  cargo test -p farik-store --test event_log_file keeps_the_board
  # expected: FAIL
  # assertion `left == right` failed: the board is in the log's own file
  #   left: 0
  #  right: 1
  ```

  The board is rebuilt from the log on open either way, so nothing the test reads back through
  `Projections` can tell the two apart — only the query through another connection to the file
  can. (Run against the whole suite the same three edits also fail
  `refuses_a_projected_row_it_cannot_read_back`, which writes through the log's own connection
  and reads through `Projections`; the command above is scoped to the one test on purpose.)
  Undo all three edits before committing; nothing else depends on them.

- [x] Commit: `feat(store): catch the projections up and take one event once`

### Task 5: Building the board again from the log

Files: modified `crates/store/src/projections.rs`, `docs/plans/phase-2-protocol-store-cli/step-03-projections.md`; tested by `crates/store/src/projections.rs`

Consumes: everything above
Produces: `Projections::rebuild`

- [x] Write the failing test. Append to the tests module of `crates/store/src/projections.rs`, a blank line between it and the test above it:

  ```rust
      #[test]
      fn builds_the_board_again_from_the_log_when_it_is_told_to() {
          // Nothing in the tables is a source of truth, so this is the repair for a row that drifted
          // for any reason at all.
          let (log, projections) = a_board();
          record(&log, &projections, &about(EventKind::TaskCreated, "FRK-1"));
          log.connection()
              .execute(
                  "UPDATE task_projections SET title = 'something else', status = 'accepted'
                   WHERE task_id = 'FRK-1'",
                  (),
              )
              .expect("a row is changed by hand");
          log.connection()
              .execute(
                  "INSERT INTO task_projections
                       (task_id, kind, parent, title, status, risk, triaged, locked, updated_seq)
                   VALUES ('FRK-7', 'task', NULL, 'never happened', 'draft', 'low', 0, 0, 1)",
                  (),
              )
              .expect("and a row is invented");
          projections.rebuild().expect("the board is built again");
          let board = projections.board().expect("the board reads");
          assert_eq!(ids_of(&board), ["FRK-1"], "the invented row is gone");
          assert_eq!(board[0].title, "Add a login page");
          assert_eq!(board[0].status, TaskStatus::Draft);
          assert_eq!(projections.cursor().expect("the cursor reads"), 1);
      }
  ```

- [x] Run it and confirm it fails because there is nothing to rebuild with:

  ```
  cargo test -p farik-store --lib
  # expected: FAIL to compile,
  # error[E0599]: no method named `rebuild` found for struct `Projections` in the current scope
  ```

- [x] Write the minimal implementation. In `crates/store/src/projections.rs`, say in the `Projections` doc comment what the repair is, replacing

  ```rust
  /// The projections of one log: derived tables that answer a view in one query.
  ///
  /// They share the log's connection and its lock, so a view cannot read a half-written append, and
  /// a log opened in memory can be projected at all.
  pub struct Projections {
      log: Arc<EventLog>,
  }
  ```

  with:

  ```rust
  /// The projections of one log: derived tables that answer a view in one query.
  ///
  /// Dropping them and replaying the log is always correct, which is what `rebuild` does. They share
  /// the log's connection and its lock, so a view cannot read a half-written append, and a log opened
  /// in memory can be projected at all.
  pub struct Projections {
      log: Arc<EventLog>,
  }
  ```

  and insert into `impl Projections`, before `apply`:

  ```rust
      /// Throws the projections away and builds them again from the whole log.
      ///
      /// This is the repair: nothing in the tables is a source of truth, so a projection that has
      /// drifted for any reason — a bug fixed since, a row changed by hand, a migration that added a
      /// column — is corrected by reading the log again.
      ///
      /// # Errors
      ///
      /// `Sqlite` when the tables cannot be cleared or written; `InvalidEvent` when the log holds a
      /// row that is not an event.
      pub fn rebuild(&self) -> Result<(), StoreError> {
          {
              let mut connection = self.connection();
              let transaction = connection.transaction()?;
              transaction.execute("DELETE FROM task_projections", ())?;
              write_cursor(&transaction, 0)?;
              transaction.commit()?;
          }
          self.catch_up()
      }
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 30 passed (the store's modules)
  #           test result: ok. 7 passed (event_log_file)
  ```

- [x] Run the format and lint checks:

  ```
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  # expected: both silent
  ```

- [x] Commit: `feat(store): build the board again from the log`

### Task 6: The plans say what the projections became

Files: modified `docs/plans/project-plan.md`, `docs/plans/phase-2-protocol-store-cli/step-03-projections.md`

Consumes: everything above
Produces: a project plan that describes the projections as they now are

This task changes documentation and has no test cycle. The `> ` marker on each block below is this plan's and is not part of the text to write.

- [x] In `docs/plans/project-plan.md`, in the phase 2 section, replace the line beginning `- Step 03: \`struct TaskProjection\`` with:

  > - Step 03 (`farik-store::projections`): `struct TaskProjection { task_id: TaskId, kind: TaskKind, parent: Option<TaskId>, title: String, status: TaskStatus, risk: Risk, triaged: bool, locked: bool, updated_seq: u64 }` — the fields an event of this phase carries (changed 2026-09-17 by the step 03 plan: `assignee_id`, `reviewer_id`, `sprint_id` and `iteration` arrive with `task.transitioned` in phase 3 step 03, which is the event that carries them, as `cost_usd` arrives in 3.09 and `waiting_on_human`, `awaiting_approval` and `awaiting_integration` in 3.10; a column nothing can write is a column no test can hold to anything); `fn open_projections(log: Arc<EventLog>) -> Result<Projections, StoreError>` (an `Arc` rather than a reference, changed 2026-09-17 by the step 03 plan, because `rebuild` and the catch-up on open both read the log and phase 3 holds the two side by side, which is also what lets `rebuild(&self)` keep this signature); `impl Projections { fn rebuild(&self) -> Result<(), StoreError>; fn apply(&self, event: &FarikEvent) -> Result<(), StoreError>; fn board(&self) -> Result<Vec<TaskProjection>, StoreError>; fn task(&self, id: &TaskId) -> Result<Option<TaskProjection>, StoreError>; fn cursor(&self) -> Result<u64, StoreError> }` (opening catches up from the cursor; `apply` ignores an event at or before it, and moves the row and the cursor in one transaction; `rebuild` resets and replays). `farik-core` gains the alias `TaskKind` for `FarikTaskContractKind`. The projections live in the log's database and share its connection and lock, because a log at `:memory:` cannot be reached by a second connection. `enum CostScope`, `struct CostProjection` and `Projections::costs` move to phase 3 step 09, where `cost.recorded` arrives: no event of this phase carries a cost, the first thing that reads one is phase 3 step 09's own `budget_state`, which arrives beside the event, and no view until phase 5 step 04.

- [x] In `docs/plans/project-plan.md`, append to the end of the phase 3 step 03 interface line — the line that begins `- Step 03:` and goes on to name `enum ToolError` — as a new sentence after its closing full stop:

  > `TaskProjection` also gains `assignee_id`, `reviewer_id`, `sprint_id` and `iteration` here, from `task.transitioned` (moved 2026-09-17 from phase 2 step 03, which had no event that carries them).

- [x] In `docs/plans/project-plan.md`, append to the end of the phase 3 step 09 interface line — the line that begins `- Step 09:` and goes on to say that `TaskProjection` gains `cost_usd` — as a new sentence after its closing full stop:

  > `enum CostScope { Task, Agent, Session, Sprint, Day }`, `struct CostProjection { scope, key, usd, input_tokens, output_tokens }` and `Projections::costs(&self, scope) -> Result<Vec<CostProjection>, StoreError>` arrive here too, with the event that feeds them (moved 2026-09-17 from phase 2 step 03, which had no such event).

- [x] In `docs/plans/project-plan.md`, in the phase 2 step table, replace the last cell of the step 03 row — `Board and cost projections rebuilt from the log and updated per event, with a cursor` — with:

  > Board projections rebuilt from the log and updated per event, with a cursor; the cost projections move to phase 3 step 09, with the event that feeds them

- [x] In `docs/plans/project-plan.md`, in the phase 3 step table, append to the last cell of the step 09 row, after `the price override`:

  > , and the cost projections moved here from phase 2 step 03

- [x] Set this plan's `Status:` to `done` and confirm every checkbox above is ticked, each in the commit of the task it belongs to.

- [x] Commit: `docs(docs): record what step 03 changed about the projections`

## Verification

- [x] The whole check, from the workspace root:

  ```
  cargo xtask check
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 225 passed (farik-core)
  #   test result: ok. 37 passed (farik-protocol)
  #   test result: ok. 30 passed (farik-store, the event_log and projections modules)
  #   test result: ok. 7 passed (crates/store/tests/event_log_file.rs)
  #   test result: ok. 24 passed (xtask)
  ```

- [x] Every commit subject is accepted:

  ```
  for subject in \
    "feat(store): open a log's projections and say how far they have read" \
    "feat(store): put a filed request on the board" \
    "feat(store): record a triage and a contract the human holds" \
    "feat(store): catch the projections up and take one event once" \
    "feat(store): build the board again from the log" \
    "docs(docs): record what step 03 changed about the projections"; do
    printf '%s\n' "$subject" > /tmp/subject && cargo xtask commit-msg /tmp/subject
  done
  # expected: silent, six times
  ```

## Open questions

none
