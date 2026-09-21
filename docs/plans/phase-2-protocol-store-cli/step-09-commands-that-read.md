# Phase 2, step 09: the commands that read

Status: in progress
Branch: `claude/phase-0-implementation-izm38y` (the harness-assigned branch this phase is on)
Spec: `docs/SPEC.md` sections 5.12, 5.13, 8.4, F3, F11, F15, F16
Depends on: steps 01 to 08 of this phase, whose last commit is `6f7b64d`

Readiness confirmed by: nobody. The user asked on 2026-09-21 for this step to be written and pushed
without the readiness and landing reviews the workflow asks for, to finish the phase quickly, and
that is what happened. Step 08's landing review did not run either: it was cut off by a rate limit.
Both are recorded here and in the pull request rather than left to be discovered.

## Goal

A person who has run `farik init` can read everything the project knows from the command line:
`farik board` shows the lifecycle, `farik task show` one contract with its events, `farik log` the
event log with filters and a JSON-lines export (F11), `farik doctor` every way the files and the log
disagree plus the four things earlier steps recorded as being nobody's to report, and `farik rules
show` and `farik criteria list` the two team files a person hand-edits (F15, F16).

## Decisions

- **Every reading command prints a table of plain text and, with `--json`, the same data as JSON.**
  Step 08 built `Report { lines, json }` for exactly this; no command grows a second output path.
- **`farik log --json` prints JSON lines**, one event per line, not one array: F11 calls it an export,
  and a log that is read by `jq` a line at a time is the format that survives being large. This is the
  one command whose `--json` is not a single object, and it says so in its help.
- **`farik doctor` writes `drift.detected` events**, one per drift, which is what D5 says reconciliation
  does. It is the only reading command that writes, and the events are the record that somebody looked.
- **`doctor` reports five things beyond drift**, all recorded by earlier steps as nobody's to report:
  a team rule whose glob or regular expression does not compile (5.12, 5.6); a key in
  `.farik/local/settings.json` that Farik does not know, because that file is the one structured file
  with no schema behind it; a criterion whose `verification` matches no branch of its `oneOf`, said in
  words rather than in the schema's; a bare repository, which reaches a person as git's own sentence
  through `ScanError::Git`; and a team file or criterion library that is there and cannot be read,
  which `farik init` refuses over but nothing else checks.
- **`doctor` exits 1 when it found something**, 0 when it did not, so a script can gate on it.
- **`ProjectFiles::list_contracts` gains the tie-break `reconcile` and `projections` already have.**
  Step 07's landing review recorded it: `FRK-01` and `FRK-1` are two spellings of one number and the
  order between them fell to `read_dir`. This is the step whose `doctor` and `board` print that list.
- **`task show` takes the contract's content from the file and its status from the log** (8.4), and
  says so in its output when the two disagree rather than picking one silently.
- **Out of scope**: editing rules or criteria from the command line (F15/F16 beyond reading, phase 5);
  `doctor --adopt` (D5), which imports file-only contracts into the log and belongs with the runtime;
  cost and metrics (F17), which need the events phase 3 emits.

## File map

```
crates/cli/src/board.rs            creates: farik board
crates/cli/src/show.rs             creates: farik task show
crates/cli/src/log.rs              creates: farik log, and the JSON-lines export
crates/cli/src/doctor.rs           creates: farik doctor and its five checks beyond drift
crates/cli/src/team.rs             creates: farik rules show and farik criteria list
crates/cli/src/lib.rs              modifies: the six subcommands and their arms
crates/cli/src/project.rs          modifies: `Project::projections`, opened once per run
crates/cli/tests/reading.rs        creates: every reading command against a real repository
crates/store/src/files.rs          modifies: list_contracts breaks its tie with the id
crates/store/tests/project_files.rs modifies: the test that pins it
docs/SPEC.md                       modifies: what the reading commands are, in section 3
docs/plans/project-plan.md         modifies: step 09's interfaces; the carried items struck off
```

## Tasks

Executed in this order, each test-first, each its own commit:

1. `list_contracts` breaks its tie with the id (`fix(store)`).
2. `farik board` and `farik task show` (`feat(cli)`).
3. `farik log`, with the JSON-lines export (`feat(cli)`).
4. `farik rules show` and `farik criteria list` (`feat(cli)`).
5. `farik doctor`, its five checks, and the `drift.detected` events (`feat(cli)`).
6. The documents (`docs(docs)`).

## Verification

- [ ] `cargo xtask check --integration` ends `xtask check: ok`.
- [ ] `cargo xtask check` leaves every test that needs git ignored.
- [ ] `cargo xtask core-io` is silent.
- [ ] The six commands run by hand on a real repository.

## Open questions

none
