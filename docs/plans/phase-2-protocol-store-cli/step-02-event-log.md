# Phase 2, step 02: The event log

Status: done
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned phase branch, left as assigned per `docs/standards/code.md`; a session may not push to another branch without permission, so phase 2 reuses it as phase 1 did; steps do not get their own)
Spec: `docs/SPEC.md` section 5.1 (the log is the source of truth), 8.4 (storage), 8.5 (the event protocol); `docs/standards/code.md`, "Wire and file formats" and "Rust integration test"
Depends on: phase 0 (merged in #4), phase 1 (merged in #5), step 01 of this phase (committed as 1f93550)

A plan is `ready` only when a reviewer other than the author has confirmed the three rules in `docs/standards/workflow.md` stage 2 (Plan): every decision made, no ambiguity, no forward dependencies. Record who confirmed and when here.

Readiness confirmed by: a fresh Claude Code review session, 2026-09-17, on the second round. It rebuilt the step outside the working tree from this plan's own fenced blocks, applied verbatim in task order, and reproduced every expected output with nothing guessed or filled in: each RED's error codes and missing items, each GREEN's counts (3/1, 9/2 with protocol at 37, 10/2, 12/3, 12/4), `cargo fmt --all --check` silent after every task, all six commit subjects accepted by `cargo xtask commit-msg` with scopes on the list in `docs/standards/code.md`, Task 6's three project-plan anchors each found exactly once, the changed-file set exactly the File map, and `cargo xtask check` ending in `xtask check: ok`.

The first round refused it on ten findings, taken in `9b9229c`; the most serious was that Task 1 wrote the error enum, the migration SQL and the applier before its first test, so two of its three assertions were green by construction. The second round's six non-blocking findings — three Task 6 anchors that described a span more loosely than they quoted it, the missing reason for hand-writing `Display` rather than adding `thiserror`, a `Produces:` line short of `applied_migrations`, and the unstated multiplicity of Task 2's red — are taken in the commit that records this line.

## Goal

Farik has somewhere to keep what happened. `farik-store` opens a SQLite database under `.farik/local/`, brings its shape up to date, and hands back an `EventLog` that appends events, reads the ones a query asks for, announces each append to whoever is listening, and hands out task ids. Every event goes in through the protocol crate's own reader, so a row that is not a valid event cannot be written; the engine itself refuses to change or delete one afterwards. The log survives the process: reopening it carries on from the sequence number and the task id it left off at. Nothing above the store exists yet — no projections, no commands — so what a user sees at the end of this step is a `.farik/local/farik.db` that a second `farik` process can append to at the same time without either losing an event or repeating a number.

## Decisions

- `open_event_log(path: &Path, now: DateTime<Utc>)` takes the clock: chose the injected time over reading the clock inside `migrations::apply` because `docs/standards/code.md` ("Time, randomness, and identifiers are injected") allows no ambient clock outside the edge, and the migration ledger stamps `applied_at`. Chose a `DateTime<Utc>` value over the `Clock` trait step 01 built, although the project plan's every-phase decision names that trait as the mechanism: a log is opened once per command and stamps one row, so there is nothing to sample repeatedly, and the trait would make the store depend on something it calls exactly once. A caller that holds a `Clock` passes `clock.now()`. The project plan's recorded signature has no `now`; task 6 records the change.
- `append(&self, event: &NewEvent)` takes a reference rather than the value the project plan records: chose the reference because `NewEvent` is not `Copy` and clippy's `needless_pass_by_value` (pedantic, denied) refuses the value form. Task 6 records the change.
- `append` re-validates through `event_from_value`: chose to check every event against `docs/schemas/event.schema.json` on the way in over trusting the caller, because `NewEvent`'s fields are public, so an event can reach the log without passing through `new_event`, and the log cannot be corrected afterwards. The placeholder sequence number zero never reaches a row; the insert assigns the real one.
- `StoreError` gains `TaskIdsExhausted { next }`: chose a refusal of its own over `Sqlite` with a message because the contract schema's `^FRK-[0-9]{1,6}$` has an end, and a caller that wants to say so needs to match on it rather than read a string.
- `farik_protocol::event::body_to_value` becomes public: chose to expose it over having the store write a body itself, because two hand-written writers of the same schema drift. No test is added in `farik-protocol` for the change: a child `mod tests` already sees a private parent item, so a test there cannot fail on visibility. What pins it is the store's use across the crate boundary, which the store's own red state shows.
- `StoreError` writes its own `Display` and `std::error::Error` rather than deriving them with `thiserror`, which `docs/standards/code.md` names for a crate's error enum: chose the hand-written impls because `thiserror` is not in `[workspace.dependencies]` and the workspace pins every version by hand, so using it would add a dependency for four messages. This is the first error enum in the workspace to carry a `Display` at all (`EventError` and `PricingError` have none); adding the crate is a change for whichever step first needs it across several crates.
- Every table is `STRICT`: chose the strict form over SQLite's default affinity rules because a column declared `TEXT` that accepts an integer lets a row mean something other than what it says, and the log is the source of truth.
- Append-only is enforced by `BEFORE UPDATE` and `BEFORE DELETE` triggers that `RAISE(ABORT)`: chose the engine over this module's discipline because `farik doctor`, a repair script, and a person with the `sqlite3` shell all reach the same table.
- Write-ahead logging and `synchronous = FULL` are set for a database on a file and not for one in memory: chose this over setting them always because a database in memory has no journal to set, and `PRAGMA journal_mode = WAL` on it is a silent no-op. Only the journal mode is asserted; `synchronous` is per connection and leaves no trace another connection can read.
- The ledger of applied migrations belongs to the applier: chose to create `schema_migrations` in `migrations::apply` over creating it in `0001_event_log.sql` because the applier reads it before it applies anything, so it has to exist first; a migration that also created it would be creating the thing that decides whether it runs.
- One `Mutex<Connection>`, recovered from poisoning with `PoisonError::into_inner`: chose a lock over a connection pool because SQLite serialises writers anyway and a lock makes the sequence number a caller is handed the one its own insert produced; chose recovery over propagating the poison because a panic elsewhere says nothing about this database, and refusing every later append would turn one bug into a stopped team.
- `rusqlite` with the `bundled` feature, pinned at `=0.40.2`: chose the bundled SQLite over the system one so that the shape of the database does not depend on which SQLite the machine has, which is what `STRICT` (3.37) and `RETURNING` (3.35) require.
- A query with no filter reads the whole log, and a query whose `after_seq` no row can hold reads nothing rather than being refused: chose the empty answer because a reader resuming from the end of the log is the ordinary case.
- Task ids come from a `task_counters` row, incremented and read in one statement (`INSERT ... ON CONFLICT DO UPDATE ... RETURNING next`) inside a transaction: chose the counter table over `max(seq)` or a scan of the contracts because the id has to be unique across processes, and two `farik` commands run at once.
- The unit tests open `:memory:`; what only a database on a file can show lives in `crates/store/tests/event_log_file.rs`: chose the split over one file because `docs/standards/code.md` puts a test that needs the file system in `tests/`, and because the unit tests reach `log.connection` to write a row by hand, which no integration test can.
- Two unit tests reach into private state, against `docs/standards/code.md`'s testing rules, and each is deliberate. `log.connection` is how a row that this crate would never write gets into the log, which is the whole subject of the two tests that use it. `log.subscribers_lock().len()` is how the subscribe test says that a departed subscriber is forgotten: sending on a closed channel already fails silently, so an `announce` that ignored the error instead of dropping the sender would pass every observable assertion while the list grew without bound for as long as the process lived.
- `crates/store/tests/event_log_file.rs` is the workspace's first `tests/` file, and it runs in the default `cargo xtask check` rather than behind `--integration`: it needs a temporary directory and nothing else — no Docker, no git binary — and the project plan's every-phase testing decision gates the flag on the first test that needs one of those (phase 2 step 04). Gating this one would have left the only tests of reopening, of two processes on one file, and of the journal mode out of every check until step 04. Task 6 amends that decision and step 04's row so the plan stops promising what this step has done.
- The integration test file has a hand-rolled `TempDir` that removes its directory on drop: chose eight lines over a `tempfile` dependency because the workspace pins every version by hand and this is the only place that needs one.
- The integration file's module doc says what the file is for rather than listing the tests in it: chosen so that the doc is written once instead of being rewritten by each task that adds a test to it.

## Design

`farik-store` is a new crate with three modules. `error` holds `StoreError`. `migrations` holds the SQL, as `include_str!` of files under `src/migrations/`, and applies each one with its ledger record in a single transaction. `event_log` holds `EventLog`, `EventQuery`, and `open_event_log`.

The row keeps the envelope in columns, because that is what the log is queried by, and the body as the canonical JSON of `docs/schemas/event.schema.json`, because its shape belongs to the kind. Nothing is stored twice: a read rebuilds the wire value from the columns and the body, hands it to `event_from_value`, and refuses the row if it does not come back. An append does the same thing on the way in, with a placeholder sequence number, before the insert assigns the real one.

Subscriptions are `std::sync::mpsc` senders, fanned out after the append has committed, so nothing hears about an event that did not happen. A subscriber that has dropped its receiver is removed at the next append.

Out of scope for this step: the projections (step 03), `.farik/` files other than the database (step 04), the command line (step 05), and any event kind beyond the nine step 01 defined. Nothing reads the log yet except its own tests.

## Architecture notes

- New: `crates/store` (`farik-store`), depending on `chrono`, `farik-core`, `farik-protocol`, `rusqlite`, `serde_json`. It is an edge crate: it does I/O, so hard rule 5 does not reach it, and `cargo xtask core-io` does not look at it.
- Consumed from `farik-protocol` (`crates/protocol/src/event.rs`, on the branch): `FarikEvent`, `EventEnvelope`, `NewEvent`, `EventKind`, `EVERY_KIND`, `event_from_value`, `fixtures::an_event_wire`, and `body_to_value`, which this step makes public.
- Consumed from `farik-core` (`crates/core/src/contract.rs`, on `main`): `TaskId`, whose `FromStr` is what decides whether a number the counter reached can still spell a task id.
- Changed in `farik-protocol`: `body_to_value` becomes `pub` with a doc comment. Nothing else in that crate moves.
- The workspace manifest gains `farik-protocol` as a path dependency and `rusqlite`; `crates/*` is already a member glob, so the crate needs no `members` entry.

## Global constraints

- `farik-core` does no I/O, and this step does not touch it.
- Wire and file formats are `snake_case`; the columns match the schema's field names exactly, so a row and a wire event spell every field the same way.
- Event kinds are `<entity>.<past_tense_verb>`; this step adds none.
- No `unwrap` or `expect` outside tests. `PoisonError::into_inner` is how the locks avoid both.
- Every value that reaches SQL is a bound parameter. The only thing a query's contents change about the SQL text is how many placeholders the `kind` list has.
- No test is skipped, ignored, or quarantined.

## File map

```
Cargo.toml                                    modifies: adds farik-protocol and rusqlite to the workspace dependencies
crates/store/Cargo.toml                       creates: the farik-store manifest
crates/store/src/lib.rs                       creates: the crate root and what it re-exports
crates/store/src/error.rs                     creates: StoreError
crates/store/src/migrations.rs                creates: the migration list, the applier, and known_versions
crates/store/src/migrations/0001_event_log.sql creates: the events table, its indexes, its append-only triggers, task_counters
crates/store/src/event_log.rs                 creates: EventLog, EventQuery, open_event_log; tested by its own tests module
crates/store/tests/event_log_file.rs          creates: what only a log on a file can show
crates/protocol/src/event.rs                  modifies: body_to_value becomes public
Cargo.lock                                    modifies (generated by cargo): the new member and rusqlite
docs/plans/project-plan.md                    modifies: records what this step changed about the interface it had recorded, and where the first tests/ file runs
docs/plans/phase-2-protocol-store-cli/step-02-event-log.md modifies: this plan, ticked as it goes
```

## Tasks

### Task 1: A log that opens, and a shape it brings up to date

Files: created `crates/store/Cargo.toml`, `crates/store/src/lib.rs`, `crates/store/src/error.rs`, `crates/store/src/migrations.rs`, `crates/store/src/migrations/0001_event_log.sql`, `crates/store/src/event_log.rs`, `crates/store/tests/event_log_file.rs`; modified `Cargo.toml`, `Cargo.lock` (by cargo), `docs/plans/phase-2-protocol-store-cli/step-02-event-log.md`; tested by `crates/store/src/event_log.rs` and `crates/store/tests/event_log_file.rs`

Consumes: nothing from this plan
Produces: `farik_store::{StoreError, EventLog, IN_MEMORY, open_event_log}`, `EventLog::applied_migrations`, `farik_store::migrations::known_versions`

The scaffolding below — the two manifests and three files holding nothing but their `//!` docs — is what a test needs in order to fail for the right reason rather than for a missing crate. No behaviour is written until after the red.

- [x] Add to the `[workspace.dependencies]` table of `Cargo.toml`, keeping it alphabetical — `farik-protocol` after `farik-core`, `rusqlite` after `regress`:

  ```toml
  farik-protocol = { path = "crates/protocol" }
  ```

  ```toml
  rusqlite = { version = "=0.40.2", features = ["bundled"] }
  ```

- [x] Create `crates/store/Cargo.toml`:

  ```toml
  [package]
  name = "farik-store"
  version.workspace = true
  edition.workspace = true
  license.workspace = true
  repository.workspace = true
  rust-version.workspace = true

  [dependencies]
  chrono.workspace = true
  farik-core.workspace = true
  farik-protocol.workspace = true
  rusqlite.workspace = true
  serde_json.workspace = true

  [lints]
  workspace = true
  ```

- [x] Create `crates/store/src/lib.rs`, declaring the three modules and re-exporting nothing yet:

  ```rust
  //! Farik's memory: the append-only event log, the projections read from it, and the files under
  //! `.farik/` (`docs/SPEC.md` sections 5.1 and 8.4).

  /// What the store refuses, and why.
  pub mod error;
  /// The event log.
  pub mod event_log;
  /// The database's shape, as SQL applied in order.
  pub mod migrations;
  ```

- [x] Create `crates/store/src/error.rs` with one line, so that the module exists and holds nothing:

  ```rust
  //! What the store refuses, and why.
  ```

- [x] Create `crates/store/src/migrations.rs` with one line, the same way:

  ```rust
  //! The database's shape, as SQL applied in order and recorded once applied.
  ```

- [x] Write the failing tests. Create `crates/store/src/event_log.rs` with the module doc:

  ```rust
  //! The event log: every action the team takes, in the order it happened, never changed afterwards
  //! (`docs/SPEC.md` sections 5.1 and 8.4).
  ```

  then append the tests module:

  ```rust
  #[cfg(test)]
  mod tests {
      use chrono::TimeZone;

      use super::{DateTime, EventLog, IN_MEMORY, Path, Utc, open_event_log};
      use crate::migrations;

      fn at(hour: u32) -> DateTime<Utc> {
          Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
              .single()
              .expect("a real hour")
      }

      fn a_log() -> EventLog {
          open_event_log(Path::new(IN_MEMORY), at(9)).expect("a log in memory opens")
      }

      #[test]
      fn opens_a_log_and_records_every_migration_it_applied() {
          let log = a_log();
          assert_eq!(
              log.applied_migrations().expect("the ledger reads"),
              migrations::known_versions()
          );
      }

      #[test]
      fn applies_a_migration_once_however_often_the_log_is_opened() {
          // Opening is done on every command, so it has to be free of consequences the second time.
          let log = a_log();
          let mut connection = log.connection.lock().expect("a fresh lock");
          migrations::apply(&mut connection, at(10)).expect("applying again is a no-op");
          drop(connection);
          assert_eq!(
              log.applied_migrations().expect("the ledger reads"),
              migrations::known_versions()
          );
      }

      #[test]
      fn refuses_to_change_or_delete_an_event() {
          // Append-only is the engine's rule, not only this module's: a repair script and a person
          // with the sqlite3 shell meet the same wall. The row goes in by hand, so the rule holds
          // from the migration that makes the trigger, before anything can append.
          let log = a_log();
          let connection = log.connection.lock().expect("a fresh lock");
          connection
              .execute(
                  "INSERT INTO events
                       (recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body)
                   VALUES ('2026-09-17T10:00:00Z', 'farik', 'farik', NULL, NULL, NULL,
                           'team.updated', '{}')",
                  (),
              )
              .expect("a row goes in");
          for sql in [
              "UPDATE events SET team_id = 'other' WHERE seq = 1",
              "DELETE FROM events WHERE seq = 1",
          ] {
              let refusal = connection.execute(sql, ()).expect_err("append-only");
              assert!(
                  refusal.to_string().contains("append-only"),
                  "{sql}: {refusal}"
              );
          }
          let rows: i64 = connection
              .query_row("SELECT count(*) FROM events", (), |row| row.get(0))
              .expect("the count reads");
          assert_eq!(rows, 1);
      }
  }
  ```

- [x] Create `crates/store/tests/event_log_file.rs`:

  ```rust
  //! What only a log on the file system can show: the directory being made, the sequence surviving a
  //! reopen, write-ahead logging, and two processes appending to one file.
  //!
  //! These need the file system, so they live here rather than in the module
  //! (`docs/standards/code.md`, "Rust integration test").

  use std::path::PathBuf;

  use chrono::{DateTime, TimeZone, Utc};
  use farik_store::open_event_log;

  /// A directory of its own, removed when the test ends however the test ends.
  struct TempDir {
      path: PathBuf,
  }

  impl TempDir {
      fn new(name: &str) -> Self {
          let path = std::env::temp_dir().join(format!(
              "farik-store-{name}-{}-{:?}",
              std::process::id(),
              std::thread::current().id()
          ));
          let _ = std::fs::remove_dir_all(&path);
          std::fs::create_dir_all(&path).expect("a directory under the temporary directory");
          Self { path }
      }

      fn db(&self) -> PathBuf {
          self.path.join(".farik").join("local").join("farik.db")
      }
  }

  impl Drop for TempDir {
      fn drop(&mut self) {
          let _ = std::fs::remove_dir_all(&self.path);
      }
  }

  fn at(hour: u32) -> DateTime<Utc> {
      Utc.with_ymd_and_hms(2026, 9, 17, hour, 0, 0)
          .single()
          .expect("a real hour")
  }

  #[test]
  fn makes_the_directory_the_log_belongs_in() {
      // `farik init` gives a path inside a repository that has no `.farik/` yet, so opening makes it.
      let directory = TempDir::new("makes-the-directory");
      let log = open_event_log(&directory.db(), at(9)).expect("the log opens");
      assert!(directory.db().exists(), "the database file was made");
      drop(log);
  }
  ```

- [x] Run them and confirm they fail because there is no log and no applier:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile, twice:
  # error[E0432]: unresolved imports `super::DateTime`, `super::EventLog`, `super::IN_MEMORY`,
  #   `super::Path`, `super::Utc`, `super::open_event_log`
  # error[E0425]: cannot find function `known_versions` in module `migrations`
  # error[E0425]: cannot find function `apply` in module `migrations`
  # error[E0425]: cannot find function `known_versions` in module `migrations`
  # error: could not compile `farik-store` (lib test) due to 4 previous errors
  # error[E0432]: unresolved import `farik_store::open_event_log`
  # error: could not compile `farik-store` (test "event_log_file") due to 1 previous error
  ```

- [x] Write the minimal implementation. Replace the one line of `crates/store/src/error.rs` with:

  ```rust
  //! What the store refuses, and why.

  use std::fmt;

  /// Why a store operation did not happen.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum StoreError {
      /// The file system refused: the directory could not be made, the file could not be reached.
      Io {
          /// What failed, in the words the operating system used.
          detail: String,
      },
      /// SQLite refused: a statement failed, the database is locked, a migration did not apply.
      Sqlite {
          /// What failed, in SQLite's own words.
          detail: String,
      },
      /// A row of the log cannot be read back as an event. The log is append-only and every append
      /// goes through the protocol crate's rules, so this means the file was changed by something
      /// else, or was written by a version of Farik this one does not understand.
      InvalidEvent {
          /// Which row and what is wrong with it.
          detail: String,
      },
      /// The task id counter has passed what the contract schema's pattern can spell
      /// (`^FRK-[0-9]{1,6}$`), so the store has no id left to hand out. Refused rather than
      /// returning something that is not a task id.
      TaskIdsExhausted {
          /// The number the counter reached.
          next: u64,
      },
  }

  impl fmt::Display for StoreError {
      fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
          match self {
              Self::Io { detail } => write!(formatter, "the file system refused: {detail}"),
              Self::Sqlite { detail } => write!(formatter, "sqlite refused: {detail}"),
              Self::InvalidEvent { detail } => {
                  write!(
                      formatter,
                      "the log holds a row that is not an event: {detail}"
                  )
              }
              Self::TaskIdsExhausted { next } => write!(
                  formatter,
                  "the task id counter reached {next}, which no longer fits FRK- and six digits"
              ),
          }
      }
  }

  impl std::error::Error for StoreError {}

  impl From<rusqlite::Error> for StoreError {
      fn from(error: rusqlite::Error) -> Self {
          Self::Sqlite {
              detail: error.to_string(),
          }
      }
  }

  impl From<std::io::Error> for StoreError {
      fn from(error: std::io::Error) -> Self {
          Self::Io {
              detail: error.to_string(),
          }
      }
  }
  ```

- [x] Create `crates/store/src/migrations/0001_event_log.sql`:

  ```sql
  -- The event log and the counter that hands out task ids (docs/SPEC.md 5.1, 8.4).
  --
  -- Every table is STRICT: a column declared TEXT refuses an integer, so a row that does not mean
  -- what it says cannot be written in the first place. The log is the source of truth for what
  -- happened, and a log that accepts anything is not one. `schema_migrations` is not here: the
  -- ledger of what has been applied belongs to the applier, which makes it before it reads it.

  -- One row per event. The envelope's fields are columns, because they are what the log is queried
  -- by; the body is the canonical JSON of docs/schemas/event.schema.json, because its shape belongs
  -- to the kind rather than to the table. Nothing is stored twice: a read rebuilds the wire value
  -- from the columns and the body and hands it to the protocol crate's reader, so a row that cannot
  -- be read back is refused rather than half-built.
  CREATE TABLE events (
      seq         INTEGER PRIMARY KEY AUTOINCREMENT,
      recorded_at TEXT NOT NULL,
      team_id     TEXT NOT NULL,
      project_id  TEXT NOT NULL,
      task_id     TEXT,
      agent_id    TEXT,
      session_id  TEXT,
      kind        TEXT NOT NULL,
      body        TEXT NOT NULL
  ) STRICT;

  CREATE INDEX events_by_task ON events (task_id, seq) WHERE task_id IS NOT NULL;
  CREATE INDEX events_by_agent ON events (agent_id, seq) WHERE agent_id IS NOT NULL;
  CREATE INDEX events_by_kind ON events (kind, seq);

  -- Append-only in the engine, not only in the code above it: `farik doctor`, a migration, a repair
  -- script, and a person with the sqlite3 shell all go through these.
  CREATE TRIGGER events_refuse_update
  BEFORE UPDATE ON events
  BEGIN
      SELECT RAISE(ABORT, 'the event log is append-only: an event cannot be changed');
  END;

  CREATE TRIGGER events_refuse_delete
  BEFORE DELETE ON events
  BEGIN
      SELECT RAISE(ABORT, 'the event log is append-only: an event cannot be deleted');
  END;

  -- The next number for each task id prefix. One row, `FRK`, until a second prefix exists.
  CREATE TABLE task_counters (
      prefix TEXT PRIMARY KEY,
      next   INTEGER NOT NULL
  ) STRICT;
  ```

- [x] Replace the one line of `crates/store/src/migrations.rs` with:

  ```rust
  //! The database's shape, as SQL applied in order and recorded once applied.

  use chrono::{DateTime, SecondsFormat, Utc};
  use rusqlite::Connection;

  use crate::error::StoreError;

  /// One migration: its version and the SQL that applies it.
  struct Migration {
      version: i64,
      sql: &'static str,
  }

  /// Every migration, in the order they apply. A migration is never edited once it has shipped; a
  /// change to the shape is a new one, so that a database written by an older Farik reaches the same
  /// shape as one made today.
  const MIGRATIONS: [Migration; 1] = [Migration {
      version: 1,
      sql: include_str!("migrations/0001_event_log.sql"),
  }];

  /// Brings the database to the shape this version expects, and records what it applied. Applying to
  /// a database that is already current does nothing, so opening a log twice is not an error.
  ///
  /// Each migration and its record go in one transaction: a migration that fails leaves the database
  /// as it was rather than half-migrated, which is the state nothing knows how to repair.
  ///
  /// # Errors
  ///
  /// `Sqlite` when a migration or its record fails.
  pub(crate) fn apply(connection: &mut Connection, now: DateTime<Utc>) -> Result<(), StoreError> {
      connection.execute_batch(
          "CREATE TABLE IF NOT EXISTS schema_migrations (
              version    INTEGER PRIMARY KEY,
              applied_at TEXT NOT NULL
          ) STRICT;",
      )?;
      for migration in &MIGRATIONS {
          if is_applied(connection, migration.version)? {
              continue;
          }
          let transaction = connection.transaction()?;
          transaction.execute_batch(migration.sql)?;
          transaction.execute(
              "INSERT OR REPLACE INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
              (
                  migration.version,
                  now.to_rfc3339_opts(SecondsFormat::AutoSi, true),
              ),
          )?;
          transaction.commit()?;
      }
      Ok(())
  }

  fn is_applied(connection: &Connection, version: i64) -> Result<bool, StoreError> {
      let count: i64 = connection.query_row(
          "SELECT count(*) FROM schema_migrations WHERE version = ?1",
          (version,),
          |row| row.get(0),
      )?;
      Ok(count > 0)
  }

  /// The versions this build knows about, in order. `open_event_log` has applied every one of them.
  #[must_use]
  pub fn known_versions() -> Vec<i64> {
      MIGRATIONS
          .iter()
          .map(|migration| migration.version)
          .collect()
  }
  ```

- [x] Add the re-export of `StoreError` to `crates/store/src/lib.rs`, after the three `pub mod` lines and separated from them by a blank line:

  ```rust
  pub use error::StoreError;
  ```

- [x] Insert into `crates/store/src/event_log.rs`, between the module doc and the tests module:

  ```rust
  use std::path::Path;
  use std::sync::{Mutex, MutexGuard, PoisonError};

  use chrono::{DateTime, Utc};
  use rusqlite::Connection;

  use crate::error::StoreError;
  use crate::migrations;

  /// The path that opens a database in memory rather than on disk, for tests and for a dry run.
  pub const IN_MEMORY: &str = ":memory:";

  /// An append-only log of everything that happened, with the projections' counters beside it.
  ///
  /// One connection, behind a lock: SQLite serialises writers anyway, and a lock here means the
  /// sequence number a caller is handed is the one its own insert produced rather than another
  /// thread's. `append` takes `&self` so that the log can be shared.
  pub struct EventLog {
      connection: Mutex<Connection>,
  }
  ```

  then, after that:

  ```rust
  /// Opens the log at `path`, making its directory and bringing its shape up to date, and returns it
  /// ready to append to. `IN_MEMORY` opens a database that lives only as long as the value returned.
  ///
  /// Opening an existing log applies whatever migrations it is missing and nothing else, so opening
  /// is safe to do on every command.
  ///
  /// # Errors
  ///
  /// `Io` when the directory cannot be made; `Sqlite` when the file cannot be opened or a migration
  /// fails.
  pub fn open_event_log(path: &Path, now: DateTime<Utc>) -> Result<EventLog, StoreError> {
      let in_memory = path == Path::new(IN_MEMORY);
      if !in_memory
          && let Some(directory) = path
              .parent()
              .filter(|directory| !directory.as_os_str().is_empty())
      {
          std::fs::create_dir_all(directory)?;
      }
      let mut connection = Connection::open(path)?;
      connection.busy_timeout(std::time::Duration::from_secs(5))?;
      connection.pragma_update(None, "foreign_keys", "ON")?;
      migrations::apply(&mut connection, now)?;
      Ok(EventLog {
          connection: Mutex::new(connection),
      })
  }
  ```

  then, after that:

  ```rust
  impl EventLog {
      /// The migration versions this log has applied, in order.
      ///
      /// # Errors
      ///
      /// `Sqlite` when the ledger cannot be read.
      pub fn applied_migrations(&self) -> Result<Vec<i64>, StoreError> {
          let connection = self.connection();
          let mut statement =
              connection.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
          let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
          let mut versions = Vec::new();
          for row in rows {
              versions.push(row?);
          }
          Ok(versions)
      }

      /// The connection, recovering from a lock another thread poisoned by panicking. A panic
      /// somewhere else says nothing about this database, and refusing every later append because of
      /// it would turn one bug into a stopped team.
      fn connection(&self) -> MutexGuard<'_, Connection> {
          self.connection
              .lock()
              .unwrap_or_else(PoisonError::into_inner)
      }
  }
  ```

- [x] Add the second re-export to `crates/store/src/lib.rs`, after `pub use error::StoreError;`:

  ```rust
  pub use event_log::{EventLog, IN_MEMORY, open_event_log};
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 3 passed (the module's tests)
  #           test result: ok. 1 passed (event_log_file)
  ```

- [x] Run the format and lint checks:

  ```
  cargo fmt --all --check
  cargo clippy -p farik-store --all-targets -- -D warnings
  # expected: both silent
  ```

- [x] Commit: `feat(store): open an event log and bring its shape up to date`

### Task 2: Appending an event, and reading the ones a query asks for

Files: modified `crates/protocol/src/event.rs`, `crates/store/src/event_log.rs`, `crates/store/src/lib.rs`, `crates/store/tests/event_log_file.rs`, `docs/plans/phase-2-protocol-store-cli/step-02-event-log.md`; tested by `crates/store/src/event_log.rs` and `crates/store/tests/event_log_file.rs`

Consumes: `open_event_log`, `EventLog`, `IN_MEMORY` from Task 1; `FarikEvent`, `NewEvent`, `EventEnvelope`, `EventKind`, `EVERY_KIND`, `event_from_value`, `fixtures::an_event_wire`, `body_to_value` from `crates/protocol/src/event.rs`; `TaskId` from `crates/core/src/contract.rs`
Produces: `EventLog::append`, `EventLog::read`, `farik_store::EventQuery`, `farik_protocol::event::body_to_value` as public API

- [x] Write the failing tests. Replace the tests module's imports in `crates/store/src/event_log.rs` — the four lines from `use chrono::TimeZone;` to `use crate::migrations;`, blank line included — with:

  ```rust
      use chrono::TimeZone;
      use farik_protocol::event::fixtures::an_event_wire;
      use farik_protocol::event::{EVERY_KIND, EventKind};

      use super::{
          DateTime, EventLog, EventQuery, FarikEvent, IN_MEMORY, NewEvent, Path, StoreError, Utc,
          body_to_value, event_from_value, open_event_log,
      };
      use crate::migrations;
  ```

  then add, after `fn a_log()`:

  ```rust
      /// The fixture event of one kind, ready to append: what `new_event` would have produced, built
      /// from the protocol crate's own wire fixture so that the two cannot drift.
      fn an_event(kind: EventKind) -> NewEvent {
          let event = event_from_value(&an_event_wire(kind)).expect("the fixture is schema-valid");
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

      fn kinds_of(events: &[FarikEvent]) -> Vec<EventKind> {
          events.iter().map(|event| event.body.kind()).collect()
      }
  ```

  then append these six tests to the module:

  ```rust
      #[test]
      fn appends_events_in_order_and_hands_back_the_place_each_was_given() {
          let log = a_log();
          let first = log
              .append(&an_event(EventKind::TaskCreated))
              .expect("appends");
          let second = log
              .append(&an_event(EventKind::RequestTriaged))
              .expect("appends");
          assert_eq!((first.envelope.seq, second.envelope.seq), (1, 2));
          let read = log.read(&EventQuery::default()).expect("reads");
          assert_eq!(read, vec![first, second]);
      }

      #[test]
      fn reads_back_every_kind_exactly_as_it_was_appended() {
          // The row keeps the envelope in columns and the body as JSON, so this is what says the two
          // halves go back together for every kind the phase emits.
          let log = a_log();
          let appended: Vec<FarikEvent> = EVERY_KIND
              .into_iter()
              .map(|kind| log.append(&an_event(kind)).expect("appends"))
              .collect();
          assert_eq!(log.read(&EventQuery::default()).expect("reads"), appended);
          assert_eq!(kinds_of(&appended), EVERY_KIND.to_vec());
      }

      #[test]
      fn refuses_an_event_the_wire_rules_refuse_and_appends_nothing() {
          // `NewEvent`'s fields are public, so an event can reach the log without passing through
          // `new_event`. The log is the source of truth and cannot be corrected afterwards.
          let log = a_log();
          let mut blank = an_event(EventKind::TeamUpdated);
          blank.team_id = "  ".to_string();
          let refusal = log.append(&blank).expect_err("a blank team id is refused");
          assert!(
              matches!(&refusal, StoreError::InvalidEvent { detail } if detail.contains("team_id")),
              "{refusal:?}"
          );
          let mut unnamed = an_event(EventKind::ContractWritten);
          unnamed.task_id = None;
          assert!(matches!(
              log.append(&unnamed),
              Err(StoreError::InvalidEvent { .. })
          ));
          assert_eq!(log.read(&EventQuery::default()).expect("reads"), Vec::new());
      }

      #[test]
      fn reads_the_events_a_query_asks_for_and_no_others() {
          let log = a_log();
          for kind in [
              EventKind::TaskCreated,
              EventKind::RequestTriaged,
              EventKind::TeamUpdated,
              EventKind::ContractWritten,
          ] {
              log.append(&an_event(kind)).expect("appends");
          }
          let after = EventQuery {
              after_seq: Some(2),
              ..EventQuery::default()
          };
          assert_eq!(
              kinds_of(&log.read(&after).expect("reads")),
              [EventKind::TeamUpdated, EventKind::ContractWritten]
          );
          let by_kind = EventQuery {
              kinds: vec![EventKind::TaskCreated, EventKind::ContractWritten],
              ..EventQuery::default()
          };
          assert_eq!(
              kinds_of(&log.read(&by_kind).expect("reads")),
              [EventKind::TaskCreated, EventKind::ContractWritten]
          );
          let limited = EventQuery {
              limit: Some(1),
              ..EventQuery::default()
          };
          assert_eq!(
              kinds_of(&log.read(&limited).expect("reads")),
              [EventKind::TaskCreated]
          );
          // The fixture names the contract on every kind that is about one, and `team.updated` is
          // not about one, so it is the one the filter leaves out.
          let by_task = EventQuery {
              task_id: Some("FRK-1".parse().expect("a task id")),
              ..EventQuery::default()
          };
          assert_eq!(
              kinds_of(&log.read(&by_task).expect("reads")),
              [
                  EventKind::TaskCreated,
                  EventKind::RequestTriaged,
                  EventKind::ContractWritten
              ]
          );
          let other_task = EventQuery {
              task_id: Some("FRK-2".parse().expect("a task id")),
              ..EventQuery::default()
          };
          assert_eq!(log.read(&other_task).expect("reads"), Vec::new());
          // Every filter at once, and then a sequence number the log cannot reach: a reader resuming
          // from the end asks for nothing and is told nothing, rather than refused.
          let everything = EventQuery {
              after_seq: Some(1),
              task_id: Some("FRK-1".parse().expect("a task id")),
              agent_id: None,
              kinds: vec![EventKind::ContractWritten],
              limit: Some(10),
          };
          assert_eq!(
              kinds_of(&log.read(&everything).expect("reads")),
              [EventKind::ContractWritten]
          );
          let past_the_end = EventQuery {
              after_seq: Some(u64::MAX),
              ..EventQuery::default()
          };
          assert_eq!(log.read(&past_the_end).expect("reads"), Vec::new());
      }

      #[test]
      fn reads_the_events_one_agent_produced() {
          let log = a_log();
          let mut by_maya = an_event(EventKind::TeamUpdated);
          by_maya.agent_id = Some("maya-chen".to_string());
          log.append(&by_maya).expect("appends");
          log.append(&an_event(EventKind::TaskCreated))
              .expect("appends");
          let query = EventQuery {
              agent_id: Some("maya-chen".to_string()),
              ..EventQuery::default()
          };
          assert_eq!(
              kinds_of(&log.read(&query).expect("reads")),
              [EventKind::TeamUpdated]
          );
          let nobody = EventQuery {
              agent_id: Some("nobody".to_string()),
              ..EventQuery::default()
          };
          assert_eq!(log.read(&nobody).expect("reads"), Vec::new());
      }

      #[test]
      fn refuses_a_row_that_is_not_an_event_rather_than_half_reading_it() {
          // Nothing this crate writes can produce such a row, so this is about the file having been
          // changed by something else, or written by a Farik this one does not understand.
          let log = a_log();
          let body = body_to_value(
              &event_from_value(&an_event_wire(EventKind::TaskCreated))
                  .expect("the fixture")
                  .body,
          )
          .to_string();
          log.connection
              .lock()
              .expect("a fresh lock")
              .execute(
                  "INSERT INTO events
                       (recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body)
                   VALUES ('2026-09-17T10:00:00Z', 'farik', 'farik', 'FRK-1', NULL, NULL,
                           'contract.locked', ?1)",
                  (body,),
              )
              .expect("the row is written by hand");
          let refusal = log
              .read(&EventQuery::default())
              .expect_err("a body that does not fit its kind");
          assert!(
              matches!(&refusal, StoreError::InvalidEvent { detail } if detail.contains("event 1")),
              "{refusal:?}"
          );
      }
  ```

- [x] Grow `crates/store/tests/event_log_file.rs`. Replace its whole import section — the four lines from `use std::path::PathBuf;` to `use farik_store::open_event_log;`, the blank line between them included — with:

  ```rust
  use std::path::PathBuf;

  use chrono::{DateTime, TimeZone, Utc};
  use farik_protocol::event::fixtures::an_event_wire;
  use farik_protocol::event::{EventKind, NewEvent, event_from_value};
  use farik_store::{EventQuery, open_event_log};
  ```

  add, after `fn at`:

  ```rust
  fn an_event(kind: EventKind) -> NewEvent {
      let event = event_from_value(&an_event_wire(kind)).expect("the fixture is schema-valid");
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
  ```

  replace the two lines of `makes_the_directory_the_log_belongs_in` that end it —

  ```rust
      assert!(directory.db().exists(), "the database file was made");
      drop(log);
  ```

  — with:

  ```rust
      assert!(directory.db().exists(), "the database file was made");
      log.append(&an_event(EventKind::TaskCreated))
          .expect("appends");
  ```

  and append:

  ```rust
  #[test]
  fn keeps_every_event_and_its_place_across_a_reopen() {
      let directory = TempDir::new("survives-a-reopen");
      {
          let log = open_event_log(&directory.db(), at(9)).expect("the log opens");
          log.append(&an_event(EventKind::TaskCreated))
              .expect("appends");
          log.append(&an_event(EventKind::RequestTriaged))
              .expect("appends");
      }
      let reopened = open_event_log(&directory.db(), at(10)).expect("the log opens again");
      let read = reopened.read(&EventQuery::default()).expect("reads");
      assert_eq!(
          read.iter()
              .map(|event| event.envelope.seq)
              .collect::<Vec<u64>>(),
          [1, 2]
      );
      // The next place carries on from where the log left off; a reused sequence number would make
      // two different events look like one.
      let third = reopened
          .append(&an_event(EventKind::TeamUpdated))
          .expect("appends");
      assert_eq!(third.envelope.seq, 3);
  }
  ```

- [x] Run them and confirm they fail because appending and reading are missing:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile, twice. These codes and no others, each `E0599` once per call
  # site (22 errors in the lib test, 6 in the integration test):
  # error[E0432]: unresolved imports `super::EventQuery`, `super::FarikEvent`, `super::NewEvent`,
  #   `super::body_to_value`, `super::event_from_value`
  # error[E0432]: unresolved import `farik_store::EventQuery`
  # error[E0599]: no method named `append` found for struct `EventLog` in the current scope
  # error[E0599]: no method named `read` found for struct `EventLog` in the current scope
  ```

- [x] Make the body writer public. In `crates/protocol/src/event.rs`, replace the line

  ```rust
  fn body_to_value(body: &EventBody) -> Value {
  ```

  with:

  ```rust
  /// One body as the canonical wire value its kind's schema describes. The store holds this rather
  /// than the whole event, because the envelope's fields are the log's own columns.
  #[must_use]
  pub fn body_to_value(body: &EventBody) -> Value {
  ```

- [x] Write the minimal implementation. In `crates/store/src/event_log.rs`, replace the import block — the eight lines from `use std::path::Path;` to `use crate::migrations;` — with:

  ```rust
  use std::fmt::Write;
  use std::path::Path;
  use std::sync::{Mutex, MutexGuard, PoisonError};

  use chrono::{DateTime, SecondsFormat, Utc};
  use farik_core::contract::TaskId;
  use farik_protocol::event::{
      EventEnvelope, EventKind, FarikEvent, NewEvent, body_to_value, event_from_value,
  };
  use rusqlite::types::Value as SqlValue;
  use rusqlite::{Connection, params_from_iter};
  use serde_json::{Map, Value};

  use crate::error::StoreError;
  use crate::migrations;
  ```

  insert after the `EventLog` struct and before `open_event_log`:

  ```rust
  /// Which events to read. Every field left empty means "no filter"; `Default` reads the whole log.
  #[derive(Debug, Clone, Default, PartialEq, Eq)]
  pub struct EventQuery {
      /// Only events after this sequence number, exclusive, which is how a reader resumes.
      pub after_seq: Option<u64>,
      /// Only events about this contract.
      pub task_id: Option<TaskId>,
      /// Only events produced by this agent.
      pub agent_id: Option<String>,
      /// Only these kinds; an empty list is every kind.
      pub kinds: Vec<EventKind>,
      /// At most this many, taken from the lowest sequence number up.
      pub limit: Option<usize>,
  }
  ```

  insert into `impl EventLog`, before `applied_migrations`:

  ```rust
      /// Appends one event and returns it with the sequence number the log gave it.
      ///
      /// The event is checked against `docs/schemas/event.schema.json` on the way in, by the same
      /// reader that guards the wire, because `NewEvent`'s fields are public and a caller that built
      /// one by hand rather than through `new_event` would otherwise put a blank id or a body that
      /// does not match its kind into a log nothing can correct.
      ///
      /// # Errors
      ///
      /// `InvalidEvent` when the event does not pass that check; `Sqlite` when the insert fails.
      pub fn append(&self, event: &NewEvent) -> Result<FarikEvent, StoreError> {
          // Sequence zero is a placeholder that never reaches a row: the insert assigns the real one,
          // and the schema's `seq` allows zero only so that this check can be made before it exists.
          let checked = read_wire(&wire_of(event, 0)).map_err(|detail| StoreError::InvalidEvent {
              detail: format!("an event was refused before it was appended: {detail}"),
          })?;
          let body = body_to_value(&checked.body).to_string();
          let seq = {
              let connection = self.connection();
              connection.execute(
                  "INSERT INTO events
                       (recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                  (
                      stamp(checked.envelope.recorded_at),
                      &checked.envelope.team_id,
                      &checked.envelope.project_id,
                      checked.envelope.task_id.as_ref().map(|id| id.to_string()),
                      checked.envelope.agent_id.as_ref(),
                      checked.envelope.session_id.as_ref(),
                      checked.body.kind().to_string(),
                      body,
                  ),
              )?;
              u64::try_from(connection.last_insert_rowid()).map_err(|_| StoreError::Sqlite {
                  detail: "the log's sequence number is negative, which no append can produce"
                      .to_string(),
              })?
          };
          let appended = FarikEvent {
              envelope: EventEnvelope {
                  seq,
                  ..checked.envelope
              },
              body: checked.body,
          };
          Ok(appended)
      }
  ```

  then, still before `applied_migrations`:

  ```rust
      /// Reads the events the query asks for, in the order they happened.
      ///
      /// # Errors
      ///
      /// `Sqlite` when the read fails; `InvalidEvent` when a row cannot be read back as an event.
      pub fn read(&self, query: &EventQuery) -> Result<Vec<FarikEvent>, StoreError> {
          let Some((sql, parameters)) = statement_of(query) else {
              // A sequence number no row can hold asks for nothing, which is an empty answer rather
              // than an error: a reader resuming from the end of the log is the ordinary case.
              return Ok(Vec::new());
          };
          let connection = self.connection();
          let mut statement = connection.prepare(&sql)?;
          let rows = statement.query_map(params_from_iter(parameters), |row| {
              Ok((
                  row.get::<_, i64>(0)?,
                  row.get::<_, String>(1)?,
                  row.get::<_, String>(2)?,
                  row.get::<_, String>(3)?,
                  row.get::<_, Option<String>>(4)?,
                  row.get::<_, Option<String>>(5)?,
                  row.get::<_, Option<String>>(6)?,
                  row.get::<_, String>(7)?,
                  row.get::<_, String>(8)?,
              ))
          })?;
          let mut events = Vec::new();
          for row in rows {
              events.push(event_of_row(row?)?);
          }
          Ok(events)
      }
  ```

  and append, after the closing brace of `impl EventLog` and before the tests module:

  ```rust
  /// The wire value of an event that has not been appended, with `seq` as given.
  fn wire_of(event: &NewEvent, seq: u64) -> Value {
      let mut wire = Map::new();
      wire.insert("seq".to_string(), Value::from(seq));
      wire.insert(
          "recorded_at".to_string(),
          Value::String(stamp(event.recorded_at)),
      );
      wire.insert("team_id".to_string(), Value::String(event.team_id.clone()));
      wire.insert(
          "project_id".to_string(),
          Value::String(event.project_id.clone()),
      );
      if let Some(task_id) = &event.task_id {
          wire.insert("task_id".to_string(), Value::String(task_id.to_string()));
      }
      if let Some(agent_id) = &event.agent_id {
          wire.insert("agent_id".to_string(), Value::String(agent_id.clone()));
      }
      if let Some(session_id) = &event.session_id {
          wire.insert("session_id".to_string(), Value::String(session_id.clone()));
      }
      wire.insert(
          "kind".to_string(),
          Value::String(event.body.kind().to_string()),
      );
      wire.insert("body".to_string(), body_to_value(&event.body));
      Value::Object(wire)
  }

  fn read_wire(wire: &Value) -> Result<FarikEvent, String> {
      event_from_value(wire).map_err(|errors| {
          errors
              .iter()
              .map(|error| format!("{}: {}", error.path, error.message))
              .collect::<Vec<String>>()
              .join("; ")
      })
  }

  fn stamp(at: DateTime<Utc>) -> String {
      at.to_rfc3339_opts(SecondsFormat::AutoSi, true)
  }

  type Row = (
      i64,
      String,
      String,
      String,
      Option<String>,
      Option<String>,
      Option<String>,
      String,
      String,
  );

  fn event_of_row(row: Row) -> Result<FarikEvent, StoreError> {
      let (seq, recorded_at, team_id, project_id, task_id, agent_id, session_id, kind, body) = row;
      let body: Value = serde_json::from_str(&body).map_err(|error| StoreError::InvalidEvent {
          detail: format!("the body of event {seq} is not JSON: {error}"),
      })?;
      let mut wire = Map::new();
      let seq_number = u64::try_from(seq).map_err(|_| StoreError::InvalidEvent {
          detail: format!("event {seq} has a negative sequence number"),
      })?;
      wire.insert("seq".to_string(), Value::from(seq_number));
      wire.insert("recorded_at".to_string(), Value::String(recorded_at));
      wire.insert("team_id".to_string(), Value::String(team_id));
      wire.insert("project_id".to_string(), Value::String(project_id));
      for (field, value) in [
          ("task_id", task_id),
          ("agent_id", agent_id),
          ("session_id", session_id),
      ] {
          if let Some(value) = value {
              wire.insert(field.to_string(), Value::String(value));
          }
      }
      wire.insert("kind".to_string(), Value::String(kind));
      wire.insert("body".to_string(), body);
      read_wire(&Value::Object(wire)).map_err(|detail| StoreError::InvalidEvent {
          detail: format!("event {seq} cannot be read back: {detail}"),
      })
  }

  /// The SQL and the parameters one query needs, or `None` when the query can match no row at all.
  ///
  /// Every value is a bound parameter; the only thing the query's contents change about the SQL is
  /// how many placeholders the `kind` list has.
  fn statement_of(query: &EventQuery) -> Option<(String, Vec<SqlValue>)> {
      let mut sql = "SELECT seq, recorded_at, team_id, project_id, task_id, agent_id, session_id, \
                     kind, body FROM events"
          .to_string();
      let mut conditions: Vec<String> = Vec::new();
      let mut parameters: Vec<SqlValue> = Vec::new();
      if let Some(after_seq) = query.after_seq {
          // A sequence number no row can hold matches nothing, because `seq` is a signed integer in
          // the engine and the log cannot reach past it.
          let after_seq = i64::try_from(after_seq).ok()?;
          conditions.push(format!("seq > ?{}", parameters.len() + 1));
          parameters.push(SqlValue::Integer(after_seq));
      }
      if let Some(task_id) = &query.task_id {
          conditions.push(format!("task_id = ?{}", parameters.len() + 1));
          parameters.push(SqlValue::Text(task_id.to_string()));
      }
      if let Some(agent_id) = &query.agent_id {
          conditions.push(format!("agent_id = ?{}", parameters.len() + 1));
          parameters.push(SqlValue::Text(agent_id.clone()));
      }
      if !query.kinds.is_empty() {
          let placeholders: Vec<String> = query
              .kinds
              .iter()
              .enumerate()
              .map(|(offset, _)| format!("?{}", parameters.len() + offset + 1))
              .collect();
          conditions.push(format!("kind IN ({})", placeholders.join(", ")));
          for kind in &query.kinds {
              parameters.push(SqlValue::Text(kind.to_string()));
          }
      }
      if !conditions.is_empty() {
          sql.push_str(" WHERE ");
          sql.push_str(&conditions.join(" AND "));
      }
      sql.push_str(" ORDER BY seq");
      if let Some(limit) = query.limit {
          let _ = write!(sql, " LIMIT ?{}", parameters.len() + 1);
          parameters.push(SqlValue::Integer(i64::try_from(limit).unwrap_or(i64::MAX)));
      }
      Some((sql, parameters))
  }
  ```

- [x] Add `EventQuery` to the re-export in `crates/store/src/lib.rs`, which becomes:

  ```rust
  pub use event_log::{EventLog, EventQuery, IN_MEMORY, open_event_log};
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 9 passed (the module's tests)
  #           test result: ok. 2 passed (event_log_file)
  cargo test -p farik-protocol
  # expected: test result: ok. 37 passed, unchanged by making a function public
  ```

- [x] Run the format and lint checks:

  ```
  cargo fmt --all --check
  cargo clippy -p farik-store --all-targets -- -D warnings
  # expected: both silent
  ```

- [x] Commit: `feat(store): append events and read the ones a query asks for`

### Task 3: Every append reaches whoever is listening

Files: modified `crates/store/src/event_log.rs`, `docs/plans/phase-2-protocol-store-cli/step-02-event-log.md`; tested by `crates/store/src/event_log.rs`

Consumes: `EventLog::append` from Task 2
Produces: `EventLog::subscribe`

- [x] Write the failing test. Add `use std::sync::mpsc::TryRecvError;` as the first line of the tests module in `crates/store/src/event_log.rs`, so its imports begin

  ```rust
  #[cfg(test)]
  mod tests {
      use std::sync::mpsc::TryRecvError;

      use chrono::TimeZone;
  ```

  and append to the module:

  ```rust
      #[test]
      fn announces_every_append_to_every_subscriber_and_forgets_the_ones_that_left() {
          let log = a_log();
          let first = log.subscribe();
          let second = log.subscribe();
          let appended = log
              .append(&an_event(EventKind::TaskCreated))
              .expect("appends");
          assert_eq!(first.recv().expect("the first hears"), appended);
          assert_eq!(second.recv().expect("the second hears"), appended);
          drop(second);
          let next = log
              .append(&an_event(EventKind::RequestTriaged))
              .expect("appends");
          assert_eq!(first.recv().expect("the first still hears"), next);
          assert_eq!(log.subscribers_lock().len(), 1);
          // A subscriber hears what happened after it subscribed, not before.
          let late = log.subscribe();
          assert_eq!(late.try_recv(), Err(TryRecvError::Empty));
      }
  ```

- [x] Run it and confirm it fails because nothing is announced:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile,
  # error[E0599]: no method named `subscribe` found for struct `EventLog` in the current scope
  # error[E0599]: no method named `subscribers_lock` found for struct `EventLog` in the current scope
  ```

- [x] Write the minimal implementation. In `crates/store/src/event_log.rs`, replace

  ```rust
  use std::sync::{Mutex, MutexGuard, PoisonError};
  ```

  with:

  ```rust
  use std::sync::mpsc::{Receiver, Sender, channel};
  use std::sync::{Mutex, MutexGuard, PoisonError};
  ```

  give the struct its subscribers, so that it becomes:

  ```rust
  pub struct EventLog {
      connection: Mutex<Connection>,
      subscribers: Mutex<Vec<Sender<FarikEvent>>>,
  }
  ```

  fill them in at the end of `open_event_log`, replacing

  ```rust
      Ok(EventLog {
          connection: Mutex::new(connection),
      })
  ```

  with:

  ```rust
      Ok(EventLog {
          connection: Mutex::new(connection),
          subscribers: Mutex::new(Vec::new()),
      })
  ```

  announce the append, replacing the last two lines of `append` —

  ```rust
          Ok(appended)
      }
  ```

  — with:

  ```rust
          self.announce(&appended);
          Ok(appended)
      }
  ```

  insert into `impl EventLog`, before `applied_migrations`:

  ```rust
      /// A channel that receives every event appended after this call. Each subscriber gets its own,
      /// and one that stops listening is dropped at the next append rather than holding events for a
      /// receiver nobody owns.
      ///
      /// An event reaches a subscriber only once it is committed: a subscriber that acted on an
      /// append that then failed would have seen something that did not happen.
      pub fn subscribe(&self) -> Receiver<FarikEvent> {
          let (sender, receiver) = channel();
          self.subscribers_lock().push(sender);
          receiver
      }
  ```

  and add the two helpers after `fn connection`, which is the last item of `impl EventLog`:

  ```rust
      fn subscribers_lock(&self) -> MutexGuard<'_, Vec<Sender<FarikEvent>>> {
          self.subscribers
              .lock()
              .unwrap_or_else(PoisonError::into_inner)
      }

      fn announce(&self, event: &FarikEvent) {
          self.subscribers_lock()
              .retain(|subscriber| subscriber.send(event.clone()).is_ok());
      }
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 10 passed (the module's tests)
  #           test result: ok. 2 passed (event_log_file)
  ```

- [x] Run the lint check:

  ```
  cargo clippy -p farik-store --all-targets -- -D warnings
  # expected: silent
  ```

- [x] Commit: `feat(store): announce every append to its subscribers`

### Task 4: One task id at a time, across processes

Files: modified `crates/store/src/event_log.rs`, `crates/store/tests/event_log_file.rs`, `docs/plans/phase-2-protocol-store-cli/step-02-event-log.md`; tested by both

Consumes: `EventLog`, `open_event_log` from Task 1; `StoreError::TaskIdsExhausted` from Task 1; `TaskId` from `crates/core/src/contract.rs`
Produces: `EventLog::next_task_id`

- [x] Write the failing tests. Append to the tests module of `crates/store/src/event_log.rs`:

  ```rust
      #[test]
      fn hands_out_one_task_id_per_call_and_never_the_same_one_twice() {
          let log = a_log();
          let ids: Vec<String> = (0..3)
              .map(|_| log.next_task_id().expect("an id").to_string())
              .collect();
          assert_eq!(ids, ["FRK-1", "FRK-2", "FRK-3"]);
      }

      #[test]
      fn refuses_a_task_id_the_contract_schema_cannot_spell() {
          // The pattern is `^FRK-[0-9]{1,6}$`, so the counter has an end. Refusing is the only honest
          // answer: the alternative is handing back something that is not a task id.
          let log = a_log();
          log.connection
              .lock()
              .expect("a fresh lock")
              .execute(
                  "INSERT INTO task_counters (prefix, next) VALUES ('FRK', 999999)",
                  (),
              )
              .expect("the counter is set");
          assert_eq!(
              log.next_task_id().expect_err("the millionth id"),
              StoreError::TaskIdsExhausted { next: 1_000_000 }
          );
      }
  ```

- [x] Add the last two lines to `keeps_every_event_and_its_place_across_a_reopen` in `crates/store/tests/event_log_file.rs`, after `assert_eq!(third.envelope.seq, 3);`:

  ```rust
      // And the ids the store hands out carry on too.
      assert_eq!(reopened.next_task_id().expect("an id").to_string(), "FRK-1");
  ```

  and append to that file:

  ```rust
  #[test]
  fn never_gives_two_logs_on_one_file_the_same_place_or_the_same_task_id() {
      // Two commands can run at once, and `farik` is a separate process each time.
      let directory = TempDir::new("two-logs");
      let first = open_event_log(&directory.db(), at(9)).expect("the first log opens");
      let second = open_event_log(&directory.db(), at(9)).expect("the second log opens");
      let mut places = Vec::new();
      for _ in 0..2 {
          places.push(
              first
                  .append(&an_event(EventKind::TaskCreated))
                  .expect("appends")
                  .envelope
                  .seq,
          );
          places.push(
              second
                  .append(&an_event(EventKind::TeamUpdated))
                  .expect("appends")
                  .envelope
                  .seq,
          );
      }
      assert_eq!(places, [1, 2, 3, 4]);
      let ids = [
          first.next_task_id().expect("an id").to_string(),
          second.next_task_id().expect("an id").to_string(),
          first.next_task_id().expect("an id").to_string(),
      ];
      assert_eq!(ids, ["FRK-1", "FRK-2", "FRK-3"]);
      // Both connections see the whole log, whichever of them wrote each event.
      assert_eq!(second.read(&EventQuery::default()).expect("reads").len(), 4);
  }
  ```

- [x] Run them and confirm they fail because no id is handed out:

  ```
  cargo test -p farik-store
  # expected: FAIL to compile, twice:
  # error[E0599]: no method named `next_task_id` found for struct `EventLog` in the current scope
  #   --> crates/store/src/event_log.rs
  #   --> crates/store/tests/event_log_file.rs
  ```

- [x] Write the minimal implementation. In `crates/store/src/event_log.rs`, insert before `pub const IN_MEMORY`:

  ```rust
  /// The prefix every task id this store hands out carries, from the contract schema's pattern.
  const TASK_ID_PREFIX: &str = "FRK";

  /// The highest number `^FRK-[0-9]{1,6}$` can spell.
  const HIGHEST_TASK_NUMBER: u64 = 999_999;
  ```

  and insert into `impl EventLog`, before `applied_migrations`:

  ```rust
      /// The next task id, with the counter moved on so that no two callers get the same one.
      ///
      /// # Errors
      ///
      /// `Sqlite` when the counter cannot be read or written; `TaskIdsExhausted` when the next number
      /// no longer fits the contract schema's pattern; `InvalidEvent` never.
      pub fn next_task_id(&self) -> Result<TaskId, StoreError> {
          let mut connection = self.connection();
          let transaction = connection.transaction()?;
          let next: i64 = transaction.query_row(
              "INSERT INTO task_counters (prefix, next) VALUES (?1, 1)
               ON CONFLICT (prefix) DO UPDATE SET next = next + 1
               RETURNING next",
              (TASK_ID_PREFIX,),
              |row| row.get(0),
          )?;
          transaction.commit()?;
          let number = u64::try_from(next).map_err(|_| StoreError::Sqlite {
              detail: "the task id counter is negative, which no increment can produce".to_string(),
          })?;
          if number > HIGHEST_TASK_NUMBER {
              return Err(StoreError::TaskIdsExhausted { next: number });
          }
          format!("{TASK_ID_PREFIX}-{number}")
              .parse()
              .map_err(|_| StoreError::TaskIdsExhausted { next: number })
      }
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 12 passed (the module's tests)
  #           test result: ok. 3 passed (event_log_file)
  ```

- [x] Run the lint check:

  ```
  cargo clippy -p farik-store --all-targets -- -D warnings
  # expected: silent
  ```

- [x] Commit: `feat(store): hand out one task id at a time`

### Task 5: An append that returned survives the power going out

Files: modified `crates/store/src/event_log.rs`, `crates/store/tests/event_log_file.rs`, `docs/plans/phase-2-protocol-store-cli/step-02-event-log.md`; tested by `crates/store/tests/event_log_file.rs`

Consumes: `open_event_log` from Task 1
Produces: write-ahead logging and `synchronous = FULL` on a log that lives on a file

- [x] Write the failing test. Append to `crates/store/tests/event_log_file.rs` (`rusqlite` is already a dependency of the crate, and a crate's dependencies are available to its `tests/` targets, so nothing is added to the manifest):

  ```rust
  #[test]
  fn writes_ahead_of_the_database_file() {
      // Write-ahead logging is what makes `synchronous = FULL` affordable, and it is a property of
      // the file, so another connection can read it back. `synchronous` itself is per connection and
      // leaves no trace to assert on.
      let directory = TempDir::new("writes-ahead");
      let log = open_event_log(&directory.db(), at(9)).expect("the log opens");
      log.append(&an_event(EventKind::TaskCreated))
          .expect("appends");
      let connection = rusqlite::Connection::open(directory.db()).expect("another connection");
      let mode: String = connection
          .query_row("PRAGMA journal_mode", [], |row| row.get(0))
          .expect("the journal mode reads");
      assert_eq!(mode.to_lowercase(), "wal");
  }
  ```

- [x] Run it and confirm it fails because the journal is SQLite's default:

  ```
  cargo test -p farik-store --test event_log_file
  # expected: FAIL
  # ---- writes_ahead_of_the_database_file stdout ----
  # assertion `left == right` failed
  #   left: "delete"
  #  right: "wal"
  ```

- [x] Write the minimal implementation. In `open_event_log` in `crates/store/src/event_log.rs`, insert before the `foreign_keys` pragma:

  ```rust
      if !in_memory {
          // The log is the source of truth for what happened, so an append that returned must
          // survive the machine losing power: `FULL` is that promise, and write-ahead logging is what
          // makes it affordable. A database in memory has no journal to set.
          connection.pragma_update(None, "journal_mode", "WAL")?;
          connection.pragma_update(None, "synchronous", "FULL")?;
      }
  ```

- [x] Run the tests and confirm green:

  ```
  cargo test -p farik-store
  # expected: test result: ok. 12 passed (the module's tests)
  #           test result: ok. 4 passed (event_log_file)
  ```

- [x] Commit: `feat(store): write ahead of the database file`

### Task 6: The plans say what the store's interface became

Files: modified `docs/plans/project-plan.md`, `docs/plans/phase-2-protocol-store-cli/step-02-event-log.md`

Consumes: everything above
Produces: a project plan that describes the store as it now is

This task changes documentation and has no test cycle. The `> ` marker on each block below is this plan's and is not part of the text to write.

- [x] In `docs/plans/project-plan.md`, in the phase 2 section, replace the line beginning `- Step 02 (\`farik-store\`):` with:

  > - Step 02 (`farik-store`): `enum StoreError { Io { detail }, Sqlite { detail }, InvalidEvent { detail }, TaskIdsExhausted { next: u64 } }` (the last added 2026-09-17 by the step 02 plan: the contract schema's `^FRK-[0-9]{1,6}$` has an end, so the counter does too, and a caller that wants to say so needs to match on it rather than read a message); `IN_MEMORY: &str`, the path that opens a database in memory for tests and for a dry run; `fn open_event_log(path: &Path, now: DateTime<Utc>) -> Result<EventLog, StoreError>` (the clock is injected as a value, changed 2026-09-17 by the step 02 plan: `docs/standards/code.md` allows no ambient clock and the migration ledger stamps `applied_at`; a value rather than the `Clock` trait because a log is opened once per command and stamps one row); `impl EventLog { fn append(&self, event: &NewEvent) -> Result<FarikEvent, StoreError>; fn read(&self, query: &EventQuery) -> Result<Vec<FarikEvent>, StoreError>; fn subscribe(&self) -> Receiver<FarikEvent>; fn next_task_id(&self) -> Result<TaskId, StoreError>; fn applied_migrations(&self) -> Result<Vec<i64>, StoreError> }` backed by a `task_counters` table (`append` takes a reference, changed 2026-09-17 by the step 02 plan, because clippy's `needless_pass_by_value` refuses the value form, and it re-validates every event through `event_from_value` because `NewEvent`'s fields are public); `struct EventQuery { after_seq: Option<u64>, task_id: Option<TaskId>, agent_id: Option<String>, kinds: Vec<EventKind>, limit: Option<usize> }`, whose `Default` reads the whole log; `fn migrations::known_versions() -> Vec<i64>`. `farik_protocol::event::body_to_value` becomes public in this step, so that the store does not write a body of its own.

- [x] In `docs/plans/project-plan.md`, in "Decisions that apply to every phase", replace, in the "Tests are split in three" bullet, the sentence beginning `Integration tests (\`crates/<name>/tests/<subject>.rs\`)` and ending `(phase 2 step 04).`, its closing full stop included and the sentences around it left alone, with:

  > Integration tests (`crates/<name>/tests/<subject>.rs`) need Docker, a git binary, or the file system in ways a unit test must not. One that needs only a temporary directory runs in the default `cargo xtask check`; one that needs Docker or a git binary runs by `cargo xtask check --integration`, which runs in CI as a second job of the same `check` workflow from the step that adds the first test needing it (phase 2 step 04). Changed 2026-09-17 by the phase 2 step 02 plan: the first `tests/` file is the event log's, it needs a temporary directory and nothing else, and gating it would have left reopening, two processes on one file, and the journal mode out of every check until step 04.

- [x] In `docs/plans/project-plan.md`, in the phase 2 step table, replace, in the step 04 row, the text `the first integration test and the CI job for \`cargo xtask check --integration\`` — the tail of its last cell, whose list of deliverables before the semicolon stays as it is — with:

  > the first integration test that needs a git binary, and the CI job for `cargo xtask check --integration`

- [x] In `docs/plans/project-plan.md`, in "Decisions that apply to every phase", replace, in the "Time, randomness, and identifiers are injected" bullet, the sentence beginning `Nothing in \`core\`, \`protocol\`, \`store\`, or \`runtime\`` and ending `are passed in.`, its closing full stop included, with:

  > Nothing in `core`, `protocol`, `store`, or `runtime` reads the clock or generates an identifier on its own; a `Clock` trait (`fn now(&self) -> DateTime<Utc>`) and an `IdSource` trait (`fn session_id(&self) -> String`) are passed in, or, where a call samples the clock exactly once, the `DateTime<Utc>` itself — as `open_event_log(path, now)` takes it, recorded 2026-09-17 by the phase 2 step 02 plan.

- [x] Set this plan's `Status:` to `done` and confirm every checkbox above is ticked, each in the commit of the task it belongs to.

- [x] Commit: `docs(docs): record what step 02 changed about the store`

## Verification

- [x] The whole check, from the workspace root:

  ```
  cargo xtask check
  # expected: ends with `xtask check: ok`, with
  #   test result: ok. 225 passed (farik-core)
  #   test result: ok. 37 passed (farik-protocol)
  #   test result: ok. 12 passed (farik-store, the event_log module)
  #   test result: ok. 4 passed (crates/store/tests/event_log_file.rs)
  #   test result: ok. 24 passed (xtask)
  ```

- [x] Every commit subject is accepted:

  ```
  for subject in \
    "feat(store): open an event log and bring its shape up to date" \
    "feat(store): append events and read the ones a query asks for" \
    "feat(store): announce every append to its subscribers" \
    "feat(store): hand out one task id at a time" \
    "feat(store): write ahead of the database file" \
    "docs(docs): record what step 02 changed about the store"; do
    printf '%s\n' "$subject" > /tmp/subject && cargo xtask commit-msg /tmp/subject
  done
  # expected: silent, six times
  ```

## Open questions

none
