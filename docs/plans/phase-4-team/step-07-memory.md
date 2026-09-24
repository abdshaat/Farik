# Phase 4, step 07: Memory

Status: ready
Branch: `phase/4-team`
Spec: `docs/SPEC.md` sections 5.8, 5.12, 6.1, 6.3, 8.2, 8.5
Depends on: phase 3 (merged in #11); steps 01 to 06 of this phase (committed on this branch before this step starts)
Readiness confirmed by: fresh-session reviewers, 2026-09-24 (two rounds: the second on the scan refresh the first found undecided; its findings folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

Agents remember and the team decides on the record. An agent keeps its own notebook through one tool, told how full it is and asked to prune before it overflows. The Architect and the Product Manager write decisions that nobody can rewrite afterwards, and every agent can read them. The project scan that every prompt carries is kept current as work lands. `team/retro.md` is step 06's; the project plan's line for this step is corrected to say so. Out of scope: a vector store (5.8 says none), and editing memory or decisions from the desktop app (phase 5).

## Decisions

- The notebook is `.farik/agents/<id>/memory.md`, written only through `farik_write_memory { text }` (revision 11). A write replaces the whole notebook and records `memory.written { text, written_by }`: the full text, attributed to the agent, about no contract.
  - The tool is tier `read`. Every session offers it except the one-tool sessions (triage, judgment). In a conversation (step 05) and a ceremony (step 06) it joins those sessions' lists, and so does `farik_read_decisions`. `farik_write_decision` joins neither: a decision is work for a session about work.
  - A write past the cap is refused with `memory_refused: <n> tokens is past your cap of <cap>; prune it`.
  - The cap is `policy.memory_cap_tokens` (team schema, default 8000, 500 to 16000). It is measured by `farik_core::text::tokens`, which is `ceil(chars().count() / 4)`; `text` becomes `pub mod text`, and step 05's `channel_summary` counts with it.
  - An empty text is allowed and empties the notebook.
- The prompt says how full the notebook is. The `Your memory` section (ADR 0011's fifth) opens with a line Farik writes, outside the `untrusted` block: `<n> of <cap> tokens.` When `n` is past 80 percent of the cap, a session that is offered the tool gets a second sentence: ` Prune it with farik_write_memory before it reaches the cap.` The section is written even for an empty notebook, as `0 of <cap> tokens.`, so it is never blank. The notebook is cut in the prompt at `cap × 4` characters instead of 32 KiB, so a notebook within its cap is never cut. ADR 0011 gains an amendment note saying so.
- Decisions are `.farik/decisions/NNNN-<slug>.md`, written only through `farik_write_decision { title, text }`.
  - The tool is tier `read` and accepted from the Architect and the Product Manager alone (revision 11). Anyone else is refused with `decision_refused: only the Architect and the Product Manager write decisions`.
  - `NNNN` is one more than the highest number among the files, four digits; past 9999 the write is refused with `decision_refused: the project has 9999 decisions`.
  - `slug` is the title in lower case, each run of characters other than `a-z0-9` turned into one `-`, trimmed of `-`, cut at 60 characters and trimmed of `-` again, and never empty (`decision` when it would be).
  - The file is `# <NNNN>. <title>`, a blank line, `Date: <UTC date>`, `By: <agent id>`, a blank line, then the text.
  - The title is 1 to 120 characters; the text is 1 to 32,000.
- A decision is immutable. `ProjectFiles::write_decision` writes a temporary file and then hard-links it to the final name (`std::fs::hard_link`, which fails when the name exists), so a crash never leaves a half-written decision under its name. It refuses an existing name with a new `FilesError::Exists { path }`. When two writers pick the same number, the tool tries the next number, at most three times, then refuses. No tool edits a decision.
- Each decision records `decision.written { number, slug, title, written_by }`, attributed to the agent, about no contract.
- `list_decisions` takes the files named `^[0-9]{4}-[a-z0-9-]+\.md$` and ignores the rest. A file written by hand is listed with what can be read: the title from its `# ` heading (else its slug), `date` and `author` from its `Date:` and `By:` lines when present (`Option`s).
- `farik_read_decisions { number: Option<u32> }` is tier `read` and offered where `farik_read_rules` is. With no number it answers every decision's number, title, date and author, oldest first; with one, that decision's text. An unknown number is refused with `no_such_decision: <n>`. Chose a tool over the file tools because `.farik/decisions/` lives in the project's own checkout, which a task's worktree is not (5.14).
- The scan refresh runs once per integration, in `integrate_locked` (integrate.rs) after the policy's outcome, whenever this attempt appended `task.integrated`: `auto_merge`, `manual`'s `farik integrate`, a pull request's merge (after its `fetch_fast_forward`), and a branch the human merged by hand. `Merged` carries its outcome; an `Escalated` that follows a merge (a failed push or fast-forward) gains `scan: Option<ScanRefresh>` and carries it too; the already-integrated early return is `Merged { sha, scan: Unchanged }` with no rescan. Its events carry the task's id, so `farik integrate`'s reply lists them; `criteria.updated`'s `updated_by` is `governor`; its time is `ToolDeps.clock`'s.
  - It scans only when the project's own checkout is on the integration branch (`current_branch`; a detached head or a failed read is a skip with "the checkout is not on a branch"), because `scan_project` reads the working tree and `git ls-files` at the root, which is what landed only then. Otherwise it skips, and says so.
  - What it compares is `material(read_back, criterion_names)`: the read-back without its last `, `-joined part when that part is `last commit …` or `no commits yet`, plus the criteria's names; applied to the new scan (its read-back and `names_of` its criteria) and to the last `project.scanned`'s body, read by kind. A detected command that changes under the same name is not a change; accepted.
  - When that differs from the last `project.scanned`'s, it writes `project.md` through `project_document(scan, library)` (moved with `names_of` from `crates/cli/src/init.rs` to `farik_store::scan`, where `farik init` also calls them), records `project.scanned`, and reseeds the criterion library with `seeded_library` as `farik init` does, recording `criteria.updated` when the library changed.
  - A scan or write that fails changes nothing and never fails the integration.
  - Its outcome is `ScanRefresh::{Refreshed, Unchanged, Skipped { why }, Failed { error }}`, carried by `IntegrationOutcome::Merged { sha, scan }`. It is said after the integration's own words, in the tick's `what` and in `farik integrate`'s reply: "; the project scan was refreshed", nothing when unchanged, or "; the project scan was not refreshed: <why>".
  - No model is involved (revision 11).

## File map

```
docs/schemas/event.schema.json, docs/schemas/team.schema.json   modifies: memory.written, decision.written; policy.memory_cap_tokens
crates/core/src/team.rs                          modifies: memory_cap_tokens with its default
crates/protocol/src/{event.rs,lib.rs,event/fixtures.rs}   modifies
crates/store/src/files.rs                        modifies: write_decision, list_decisions, read_decision; tests
crates/runtime/src/tools.rs, crates/runtime/src/tools/memory.rs, crates/runtime/src/tools/refusal.rs, crates/runtime/src/daemon/mcp.rs   modifies / creates: farik_write_memory, farik_write_decision, farik_read_decisions
crates/runtime/src/prompt.rs, crates/runtime/src/orchestrator/session.rs   modifies: the memory section's line; the cap passed in
crates/runtime/src/orchestrator/integrate.rs     modifies: the scan refresh in record_integrated, ScanRefresh, IntegrationOutcome::Merged.scan
crates/store/src/scan.rs, crates/cli/src/init.rs   modifies: material, project_document moved into the store
crates/core/src/lib.rs, crates/core/src/text.rs, crates/runtime/src/channel.rs   modifies: pub mod text, tokens; the summary counts with it
crates/runtime/src/orchestrator/{rules,session}.rs, crates/runtime/src/ceremonies.rs   modifies: the conversation and ceremony tool lists gain farik_write_memory and farik_read_decisions
docs/decisions/0011-the-order-of-a-session-prompt.md   modifies: an amendment note on the memory section
docs/SPEC.md, docs/plans/project-plan.md         modifies
```

## Interfaces

Consumes: `ProjectFiles::{read_memory, write_memory, write_project_scan, read_project_scan}`, `scan_project` (store), `SessionAsk::tools`, the tools' `Call`, `integrate` (runtime).

Produces:

```rust
// farik-protocol: EventBody::{MemoryWritten(MemoryWrittenBody { text, written_by }), DecisionWritten(DecisionWrittenBody { number: u32, slug, title, written_by })}
// farik-core::team: TeamPolicy::memory_cap_tokens: u32 (default 8000)
pub fn tokens(text: &str) -> u32; // ceil(chars().count() / 4), in farik-core::text (made pub)
// farik-store: FilesError::Exists { path: String }; pub fn material(read_back: &str, criterion_names: &[String]) -> String; pub fn project_document(scan: &ProjectScan, library: &CriterionLibrary) -> String
// farik-runtime::orchestrator: pub enum ScanRefresh { Refreshed, Unchanged, Skipped { why: String }, Failed { error: String } }; IntegrationOutcome::Merged { sha, scan: ScanRefresh }, IntegrationOutcome::Escalated gains scan: Option<ScanRefresh>
// farik-store
pub struct DecisionEntry { pub number: u32, pub slug: String, pub title: String, pub date: Option<NaiveDate>, pub author: Option<String> }
impl ProjectFiles {
    pub fn write_decision(&self, title: &str, text: &str, author: &str, date: NaiveDate) -> Result<DecisionEntry, FilesError>;
    pub fn list_decisions(&self) -> Result<Vec<DecisionEntry>, FilesError>;
    pub fn read_decision(&self, number: u32) -> Result<String, FilesError>;
}
// farik-runtime::prompt: PromptInput gains memory_cap_tokens: u32
```

## Tasks

### Task 1: the notebook

Files: team schema, `team.rs`, `crates/core/src/text.rs`, event schema (`memory.written`), protocol, `tools.rs`, `tools/memory.rs`, `tools/refusal.rs`, `daemon/mcp.rs`
- `counts_tokens_as_a_quarter_of_the_characters` (core) — 0 characters: 0; 1: 1; 8: 2; 9: 3.
- `writes_an_agents_memory` — `farik_write_memory { text: "use pnpm" }`: `.farik/agents/dev-a/memory.md` is that text; `memory.written { text, written_by: dev-a }`.
- `refuses_a_memory_past_its_cap` — a cap of 500 tokens and 2,001 characters: refused `memory_refused` naming 501 and 500; the file unchanged.
- `empties_a_memory` — an empty text: the file is empty, the event recorded.
- `offers_no_memory_to_a_one_tool_session` (guard) — a triage session's tools do not include it.
- `defaults_the_memory_cap` (core) — a team file without it: 8000.

- [x] `feat(runtime): let an agent keep its notebook within a cap`

### Task 2: how full the notebook is

Files: `prompt.rs`, `orchestrator/session.rs`
- `says_how_full_the_memory_is` — a notebook of 400 characters and a cap of 8000: the section opens `100 of 8000 tokens.` outside the untrusted block.
- `asks_to_prune_past_eighty_percent` — 6,404 tokens of 8,000: the line adds the prune sentence; at exactly 6,400, it does not.
- `leaves_out_a_section_with_nothing_in_it` (prompt.rs, changed) — an empty notebook now writes `Your memory` with `0 of 8000 tokens.` and no untrusted block; the other empty sections are still left out.
- `cuts_the_memory_at_its_cap` — a cap of 500 and a notebook of 2,100 characters: the untrusted block holds the first 2,000 and the cut line.

- [x] `feat(runtime): tell an agent how full its notebook is`

### Task 3: decisions

Files: event schema (`decision.written`), protocol, `files.rs`, `tools/memory.rs`, `tools.rs`, `tools/refusal.rs`, `daemon/mcp.rs`
- `writes_a_numbered_decision` (store) — the first `write_decision("Use SQLite for the log", …)`: `.farik/decisions/0001-use-sqlite-for-the-log.md` with its heading, date, and author; a second is `0002-…`.
- `never_overwrites_a_decision` (store) — a file `0003-x.md` placed by hand: the next write is 0004; a hard-link to an existing name answers `FilesError::Exists`.
- `slugs_a_title` (store) — "  Why?! Rust & Tauri  ": `why-rust-tauri`; "!!!": `decision`; a 70-character title whose 60th character falls after a space: no trailing `-`.
- `lists_a_decision_written_by_hand` (store) — `0005-x.md` holding only `hello`: listed as number 5, title `x`, no date, no author.
- `lets_the_architect_write_a_decision` — the Architect's `farik_write_decision`: the file and `decision.written { number: 1, … }`.
- `refuses_a_decision_from_a_developer` — `decision_refused`, nothing written.
- `reads_the_decisions` — two written: `farik_read_decisions {}` lists both oldest first; `{ number: 2 }` answers the second's text; `{ number: 9 }` refused `no_such_decision: 9`.

- [ ] `feat(runtime): let the architect and the product manager record decisions`

### Task 4: the scan refresh

Files: `crates/store/src/scan.rs`, `crates/cli/src/init.rs`, `orchestrator/integrate.rs`
- `measures_what_matters_in_a_scan` (store) — two scans of one tree a commit apart: `material` equal; with a new `package.json`: different.
- `rescans_after_an_integration_that_changed_the_project` (`#[ignore]`, git) — an accepted task adding a `package.json`, integrated under `auto_merge`, the checkout on the integration branch: `project.md` names the manifest; `project.scanned` after `task.integrated`; the library gains the scan's criteria and `criteria.updated` is recorded; the tick's words end "; the project scan was refreshed".
- `writes_nothing_when_the_scan_is_the_same` (`#[ignore]`, git, guard) — an integration that changes a README only: no new `project.scanned`, no words about the scan.
- `skips_the_scan_off_the_integration_branch` (`#[ignore]`, git) — the checkout on another branch under `manual`, `farik integrate`: integrated; the reply ends "; the project scan was not refreshed: the checkout is on <branch>, not <integration branch>".
- `keeps_the_integration_when_the_scan_fails` (`#[ignore]`, git) — `project.md` made a directory: the task is still integrated, and the words say the scan was not refreshed with the error.
- `init_writes_the_same_document` (cli, existing init tests) — `farik init` still writes `project.md` as before, through the moved `project_document`.

- [ ] `feat(runtime): refresh the project scan when work lands`

### Task 5: the spec

Revision 0.16 in the header, naming each change: 5.8 ("immutable once accepted" becomes immutable once written; the tool, the cap policy and how it is counted, the prompt's line and the 80 percent prompt; decisions' numbering, format, immutability, who writes them, and how agents read them; the scan refreshed after an integration); 5.12's policy row for `memory_cap_tokens` if 5.12 lists policies (else 5.8 alone); 6.1 and 6.3 (the decision tool); 8.2 (the memory section's line); 8.5 (`memory.written`, `decision.written`). The project plan's step 07 line and interface line are written as landed, the retro named as step 06's.

- [ ] `docs(docs): record memory and decisions in the spec`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok
```
