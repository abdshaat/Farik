# Standalone: shared work links repositories into one team

Status: ready
Branch: `feat/shared-work-links`, its own pull request to `main`
Decision: ADR 0085 (amends ADR 0059's linking rule)
Spec: `docs/SPEC.md` 4.4, F1 (phase 10 step 06b's linking)
Depends on: `main` at `101aaf2e` (pull request #91 merged)
Readiness confirmed by: the planner (Opus 5.5), 2026-10-23 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

When at least two of the owner's Claude Code sessions changed files in two repositories, "Do these work together?" proposes them as one team first. It says "You worked on a and b together in N sessions." and lists those sessions behind ⓘ. **Yes, one team** leads, and the owner still presses it.

File proofs keep proposing pairs with no shared work, and show as "Also" beside shared work. One shared session stays a hint.

Out of scope: any change to how sessions are read or placed; teams Catervas already drives being re-grouped.

## Decisions

- **The proof kind.** `ProofKind::SharedWork` (wire `shared_work`) is added as the enum's first variant, so the screen's order prefers it. `from_wire` and `as_wire` gain it.
- **Finding it, in core.**

  ```rust
  pub fn find_shared_work(inputs: &[ProjectHintInput], sessions: &BTreeMap<String, SessionNote>) -> Vec<Proof>
  pub struct SessionNote { pub title: Option<String>, pub last_at: DateTime<Utc> }
  pub const SHARED_SESSIONS: usize = 2;
  ```

  - For each pair, it takes `a.written_in ∩ b.written_in` (session ids, from `ProjectHintInput`). With at least `SHARED_SESSIONS`, it gives a `Proof` whose `a` and `b` are the lesser and greater keys. The sentence is "You worked on <a name> and <b name> together in <N> sessions.", with each project's display name passed in through `ProjectHintInput.name`.
  - Its `ends` are up to three `ProofEnd { project: a, file: <session id>, line: 0, text: "<title or 'A session'> · <YYYY-MM-DD>" }`, newest first. `line: 0` marks a session end; `ProofEnd`'s doc says so. When there are more than three, a fourth end reads `text: "and N more"`, with `file` empty.
  - `find_hints` keeps `WroteBoth` only when the shared count is exactly 1. `SessionNote` comes from the index's `sessions` table, `title` and `last_at`, read by the store beside `written_by_root`.
- **Where it joins.** The store's linking (`crates/store/src/linking.rs`) calls `find_shared_work` beside `find_proofs` and passes both proof lists to `find_hints`. A pair's proofs are sorted by kind, so `SharedWork` is shown first and the file proofs become "Also".
  - Asked pairs are ordered with every pair that has `SharedWork` before the rest. Within each part they keep the existing order.
  - The re-check (when shown, and in `proposal.create`) counts the shared sessions again. Below `SHARED_SESSIONS` the `shared_work` proof is dropped.
  - A No answer's stored proof list includes `shared_work`, so more shared sessions do not ask again. A new file proof kind does, as before.
- **Catervas's own sessions** never reach `written_in`, because step 02 places them `catervas` and step 06b's roots come from the owner's projects. A test holds it.
- **The screen** (`apps/web/src/pages/find/Together.tsx`). For a pair whose shown proof is `shared_work`:
  - The heading reads "These look like one team" in place of "Do these work together?". The sentence is the proof's.
  - The ⓘ lists the session ends as "<text>", with no file or line.
  - **Yes, one team** is the first button and gets focus on show. **No, keep separate** follows.
  - The line "Not sure? Keep them separate. You can change it later." stays.

  The wire's proof already carries `kind` and `ends`; nothing on the wire changes but the new kind's word.

## File map

```
crates/core/src/linking.rs                    modifies: SharedWork, find_shared_work, SessionNote, find_hints' WroteBoth rule
crates/core/src/linking/fixtures.rs           modifies: the corpus' shared-work cases
crates/store/src/linking.rs                   modifies: SessionNote from the index; shared work joined and ordered first; the re-check
docs/schemas/rpc.schema.json                  modifies: shared_work in the proof kind's enum
apps/web/src/pages/find/Together.tsx, strings/en.ts   modifies: the shared-work screen
crates/core/src/linking.rs (tests), crates/store/src/linking.rs (tests), apps/web/src/pages/find/Together.test.tsx   tests
docs/SPEC.md, docs/design/many-teams.md       modifies: the rule, revision next free
```

## Tasks

### Task 1: Shared work is a proof, in core

Tests (`crates/core/src/linking.rs`):
- `proves_shared_work_from_two_sessions`: asserts these.
  - Two projects whose `written_in` share sessions `s1` and `s2` give one `SharedWork` proof. Its sentence is "You worked on tidepool-web and tidepool-api together in 2 sessions." Its ends hold both sessions' titles with their dates, newest first.
  - With five shared sessions, there are three session ends and a fourth "and 2 more".
  - With one shared session there is no proof, and `find_hints` gives `WroteBoth`.
  - A pair with two shared sessions gives no `WroteBoth` hint.
- `proves_each_related_pair_and_no_tempting_one` (the corpus test) gains these:
  - the unrelated pairs "one session wrote both" and "sessions read both, wrote one" give no proof;
  - the related pair "two sessions wrote both" gives `SharedWork`.

- [ ] `feat(core): shared work proves two repositories are one team`

### Task 2: Shared work asked first, and checked again

Tests (`crates/store/src/linking.rs`, against an index with fixture sessions):
- `asks_shared_work_first_and_rechecks_it`: asserts these.
  - Pairs a–b (two shared sessions) and c–d (a `path_dependency`) are asked a–b first. a–b's proof is `shared_work`, with no "Also".
  - A pair with both shared work and a file proof shows `shared_work`, and the file proof's sentence is under also.
  - After one of a–b's sessions is removed from `session_roots_written`, a–b is no longer asked, and a Yes to it is `proof_no_longer_holds`.
  - A No to a–b is not asked again after a third shared session is added.
  - A session placed `catervas` that wrote under both roots adds nothing.

- [ ] `feat(store): ask about shared work first and check it again`

### Task 3: The screen

Tests (`apps/web/src/pages/find/Together.test.tsx`):
- `leads_with_yes_for_shared_work`: asserts these.
  - A pair whose proof is `shared_work` shows "These look like one team" and "You worked on tidepool-web and tidepool-api together in 2 sessions.".
  - Its ⓘ lists "Fix the login redirect · 2026-10-20" with no file or line.
  - **Yes, one team** has focus and comes first.
  - A `path_dependency` pair still shows "Do these work together?".

- [ ] `feat(web): shared work is proposed as one team`

### Task 4: Spec and design

- `docs/SPEC.md`: a new revision, the next free number above the newest, dated 2026-10-23, "records shared work as the first proof that two repositories are one team (ADR 0085)". It changes 4.4's and F1's linking rule.
- `docs/design/many-teams.md` section 4: the proof list gains shared work, first. The hints lose "a session wrote files in both" except for one session. A line cites ADR 0085.

- [ ] `docs(spec): shared work links repositories, ADR 0085`

## Verification

```
python3 reaper.py cargo xtask check
# expected: xtask check: ok
```
