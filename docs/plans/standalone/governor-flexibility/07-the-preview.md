# Governor flexibility, plan 7: the preview

Status: ready
Branch: `fix/governor-flexibility`, after plan 6
Spec and decisions: ADR 0079, "Amended after the live run" (third list); spec 0.134 amended in place

At 06:33 UTC the designer's first task, CTV-14, was escalated before any work: the preview's prepare failed, and the owner saw `governor_escalation: the preview's prepare failed:` followed by forty lines of npm's help text. The task is the one that creates the app's entry file, so the preview could not have started either.

## Decisions

- R8a (runtime): `escalation_detail` never prefixes the gate's wire name for any escalation reason; the words stand alone (plan 1 did it for budget and sessions only).
- R8b (runtime): `PreviewError`'s `Display` gives, after its first line, the error lines of the tail: lines containing `error` or `ERR` (case-insensitive) that are not option help (a line whose trimmed text starts with `--` or `-<letter>` is help), at most 8, in order; when none match, the tail's last 8 lines. The full 40-line tail stays in the session's log.
- R9 (runtime): a designer's session (`explore` or `implement` of a `ui_ux_designer` task) whose preview fails to prepare or start while the task branch has no commit of its own (`commit_count(branch, base) == 0`) runs without the preview instead of escalating. Its prompt says: "The preview did not start (<first line of the error>); this task has no commits yet, so build without it and run the app yourself when its entry point exists." Every later failure escalates as now.

## Tasks

### Task 30: an escalation says what happened, in a few lines

Files: `crates/runtime/src/transitions.rs`, `crates/runtime/src/preview.rs`
- `says_a_preview_failure_in_its_error_lines` (preview.rs) — a `Prepare` tail of npm's help text ending with `npm error The \`npm ci\` command can only install with an existing package-lock.json` displays that line and no `--allow-scripts` line, and at most 9 lines in all.
- `never_names_the_gate_in_an_escalation` (transitions.rs) — a preview escalation's channel line has no `governor_escalation`.
- [x] `fix(runtime): an escalation says what happened, in a few lines`

### Task 31: a designer's first try starts without a preview that cannot exist yet

Files: `crates/runtime/src/orchestrator/session.rs` (and the explore/implement prompt in `messages.rs`)
- `builds_without_a_preview_before_the_first_commit` (session.rs, next to the tests asserting `EscalationRaisedBodyReason::Preview`) — a designer task with no commits on its branch and a failing prepare: the session runs, no `escalation.raised` is recorded, and its prompt contains "The preview did not start". The same with one commit on the branch escalates as before.
- [x] `fix(runtime): a designer's first try starts without a preview that cannot exist yet`

### Task 32: the spec says it

Files: `docs/SPEC.md` (the preview section of 5.x and the 0.134 line)
- [x] `docs(spec): escalations say what happened, and a designer's first try needs no preview`
