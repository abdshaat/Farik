# Phase 10, step 06b: Linking with proof

Status: ready
Branch: `phase/10-many-teams`
Spec: `docs/SPEC.md` 3, 4.4, 8.4, F1; ADR 0059
Depends on: steps 06, 06c and 06d (committed)
Mockups: `ManyLinkPair` frames 1 to 3, approved with step 01
Readiness confirmed by: the planner (Opus 5.5), 2026-10-22 (ADR 0065)

This plan's own row: the project plan's row 06b ("Linking with proof, and the proposal") is split in two:

- **06b**, this plan: proofs, hints, the owner's answers and their screens.
- **06e**: the proposal, names from Claude, creating the teams, undo, fixing a mistake and suggested connections.

## Goal

Catervas finds, for every pair of found projects, the proof that one uses or deploys the other, from their committed files. It asks the owner about each pair with a proof, one per screen:

- the proof in one plain sentence;
- the files and lines behind ⓘ;
- **Yes, one team** or **No, keep separate**.

A pair with only hints is listed collapsed under "Maybe related", answered Separate unless the owner opens it.

Answers are kept: a No is never asked again unless a new proof appears. A fixture corpus of tempting unrelated pairs produces no proof.

## Decisions

### Proofs, in core (`catervas_core::linking`)

- **Input is strings.**
  - `ProjectFiles { key, name, remote_key: Option<String>, root: Option<String>, files: Vec<(path, text)> }` holds the committed files below and their text.
  - `ClaudeAiProjectText { uuid, name, description, file_names }` comes from step 04's index.

  `find_proofs(&[ProjectFiles], &[ClaudeAiProjectText]) -> Vec<Proof>` is pure.
- **Which files**, the only ones read, at the project's `HEAD` through `git show` and never the working tree:

  | Files |
  |---|
  | `package.json` (any depth to 3) |
  | `Cargo.toml` (any depth to 3) |
  | `go.mod` |
  | `.gitmodules` |
  | `docker-compose*.y*ml` and `compose*.y*ml` |
  | `*.tf` (depth to 3) |
  | `.github/workflows/*.y*ml` |
  | `k8s/**/*.y*ml`, `deploy/**/*.y*ml` (depth to 4) |
  | `vercel.json`, `netlify.toml`, `CNAME` |

  - Each file is at most 256 KiB; larger ones are skipped. At most 300 files per project are read.
  - `node_modules`, `vendor`, `target` and `.git` are never walked.
  - A folder that is not a git repository gives no files, so it has no proof and only hints.
- **The proof kinds**, each with its sentence (`a` and `b` are project names):

  | Kind | When | Sentence |
  |---|---|---|
  | `path_dependency` | a `package.json` dependency `file:` or `link:` path, a Cargo `path =` dependency or workspace member, or a `go.mod` `replace` to a local path, that resolves (relative to the file, against `a`'s root) into `b`'s root | "a uses b's code." |
  | `submodule` | a `.gitmodules` `url` whose normalised remote is `b`'s | "a includes b." |
  | `declared_package` | a dependency of `a` named as `b`'s own manifest `name`, and `b`'s manifest `repository` normalises to `b`'s remote | "a uses b's package." |
  | `deploys` | a compose `build:` or `context:` path resolving into `b`; a Terraform module `source` whose normalised remote is `b`'s; or a Kubernetes or compose `image:` that `b`'s workflows publish (a `docker/build-push-action` `tags:` or `docker push` of that image) | "a deploys b." |
  | `workflow` | a workflow `actions/checkout` with `repository: <b's owner/name>`, or a `repository_dispatch` or `workflow_dispatch` call naming it | "a's workflow works on b." |
  | `domain` | a domain `b` deploys to (its `CNAME` line, `vercel.json` `alias`, `netlify.toml` custom domain, `package.json` `homepage` host) named in `a`'s committed config files above | "a talks to b's site." |
  | `claude_ai_names` | a claude.ai Project's description or a file name holding `b`'s normalised remote or its `owner/name` | "Your claude.ai Project a is about b." |

  Each `Proof` is `{ kind, a, b, ends: [{ project, file, line, text }] /* 1 or 2 */, sentence }`. `text` is the line, trimmed to 160 characters. A pair may have several proofs; the screen shows the first by the kind order above and lists the others as "Also:".
- **Never a proof:**
  - two projects depending on the same public package, unless it is one's own (`declared_package` needs `b`'s manifest to claim it);
  - the same remote owner;
  - the same days;
  - similar names;
  - a session that read both;
  - the same service used through Claude.
- **Hints** are `find_hints(&[ProjectHintInput]) -> Vec<Hint>`, each `{ a, b, kinds: [same_owner, same_days, similar_names, wrote_both] }`:
  - `same_owner`: the same remote owner;
  - `same_days`: at least 3 days with sessions in both, within 30 days;
  - `similar_names`: a shared word of at least 4 letters after splitting names at `-`, `_` and case;
  - `wrote_both`: a session with `Write` or `Edit` tool calls on files under both roots, from step 03b's tool-call rows. Their paths are not kept, so this uses a boolean per session and root, stored by a new step: see the store below.

  A pair with any proof is not a hint pair.
- **The tempting corpus** is fixtures in `crates/core/src/linking/fixtures/`, invented projects as `ProjectFiles`. It has 8 related pairs, one per proof kind plus a second `deploys`, and 10 unrelated pairs:
  1. the same owner;
  2. the same days;
  3. similar names (`price-watch`, `watch-tower`);
  4. a session that read both;
  5. both depending on `react`;
  6. both depending on `@acme/ui`, which neither declares;
  7. both using Stripe through Claude;
  8. a compose `build: ../other` resolving outside both;
  9. a `CNAME` of one with the other's config naming a different subdomain;
  10. a `file:` dependency to a folder neither project is.

  The test holds that every related pair gives its proof and no unrelated pair gives any.
- **A proof is checked again** when shown and when a team is created: the files are read again, and `find_proofs` must still give that proof (same kind and ends' files). Otherwise it is dropped and the pair is not asked.

### The store and the hub

- **Migration 9** of `machine.db` adds:
  - `links(a TEXT NOT NULL, b TEXT NOT NULL, answer TEXT NOT NULL CHECK (answer IN ('yes','no')), proof TEXT, at TEXT NOT NULL, PRIMARY KEY (a, b)) STRICT`, with project keys and `a < b`. `proof` is the JSON of the proofs shown, kept with a Yes.
  - `session_roots_written(session_id TEXT NOT NULL, root TEXT NOT NULL, PRIMARY KEY (session_id, root)) STRICT`, filled at placement from step 03b's tool calls whose file path (read then, never kept) falls under a root. This needs `ToolUse` to carry a `writes_under: Option<root>` decided at read time. Step 03b keeps no path, so the reader computes, for each `Write`, `Edit` or `MultiEdit` call, which known root its path is under, and keeps only that root.
- **Which pairs are asked.** `linking.pairs` answers the pairs among projects chosen `team` (and teams already driven, journey B) with a proof that still holds and no answer, plus pairs answered No for which a proof kind is now found that was not in the No's stored proof list.
  - **Order:** a pair touching a project already in a confirmed group comes after its group's first pair. A pair whose two projects are already in one group through other Yes answers is not asked; its proof is listed as "Also:" on the group's screen.
  - Each pair is `{ a: { key, name, last_at, folder }, b: {…}, proof: Proof, also: [sentence], hints: [kind] }`. "Question n of N" counts these pairs.
- `linking.answer { a, b, answer: "yes"|"no" }` writes the row. A Yes needs the proof to hold now, else `proof_no_longer_holds`.
- `linking.maybe` answers the hint-only pairs among Team-chosen projects that have no answer, each `{ a, b, hints }`. Answering one is `linking.answer`. A hint pair with no answer counts as No for grouping, and is shown again only on this list.
- `linking.groups` answers the groups formed by Yes answers only (connected components over Yes), each `{ projects: [key], why: [sentence] }`. Step 06e uses it.
- A link with a driven team (journey B) records the same Yes. Step 06e turns it into adding the project to that team as a repository.

### The screens

- `/find/together`, from `ManyLinkPair`:
  - frame 1, one pair: stepper step 2; "Question n of N"; "Do these work together?"; the two projects with last worked and folder; the sentence; ⓘ listing each end as "<file>, line <n>: <text>" and "Checked again just now."; "Also: you worked on both on the same days." from hints; **Yes, one team**, **No, keep separate**; "Not sure? Keep them separate. You can change it later."
  - frame 2's "Also: it deploys tidepool-web." for `also`.
  - frame 3 after the last pair, "That's all we could prove": each group's line ("a, b and c: one team."), "Everything else stays separate.", and "Maybe related (n)" collapsed, each with its hints in words and **Separate** or **One team**.
  - **Back** and **Continue**. Continue goes to `/find/teams` (06e). Until 06e lands, it goes to All teams.
- With no pair to ask, `/find/together` shows only frame 3.

## File map

```
crates/core/src/linking.rs, linking/fixtures.rs, linking/fixtures/**   creates: find_proofs, find_hints, the corpus
crates/core/src/claude_history.rs                 modifies: ToolUse.writes_under
crates/store/src/linking.rs                       creates: reading the files at HEAD; pairs, answers, groups
crates/store/src/migrations/machine/0009_links.sql  creates
crates/store/src/claude_history.rs                modifies: session_roots_written at placement
crates/runtime/src/daemon/finding.rs              modifies: linking.pairs, answer, maybe, groups
docs/schemas/rpc.schema.json, crates/protocol, packages/protocol-client   modifies
apps/web/src/pages/find/Together.tsx               creates
docs/SPEC.md                                      modifies: revision 0.149
```

## Tasks

### Task 1: Proofs and hints, and the corpus

Tests (`crates/core/src/linking.rs`):
- `proves_each_related_pair_and_no_tempting_one`: asserts the corpus rule above, naming each failing pair.
- `finds_each_proof_s_ends`: asserts that the `path_dependency` pair's proof has both ends, `web/package.json` line 14 with its text and `api/client/package.json` line 2, and the sentence "tidepool-web uses tidepool-api's code.".
- `hints_without_proof`: asserts that `same_days`, `same_owner` and `similar_names` are found on their fixtures, and that a proof pair gives no hint.

- [x] `feat(core): prove two projects work together from their files`

### Task 2: Reading the files, and the roots a session wrote

Tests:
- `reads_only_committed_manifests_at_head` (`crates/store/tests/linking.rs`, ignored, git): asserts these.
  - From a test repository, the reader returns `package.json` and `.github/workflows/ci.yml` at `HEAD`.
  - It skips a 300 KiB `package.json` in a subfolder, and an uncommitted `compose.yaml`.
  - It never walks `node_modules`.
- `keeps_the_root_a_session_wrote_not_the_path` (`crates/store/src/claude_history.rs`): asserts these.
  - A transcript with an `Edit` of `/home/me/a/x.rs` and a `Read` of `/home/me/b/y.rs` gives `session_roots_written` of `a` only.
  - No table holds `x.rs`.

- [x] `feat(store): read committed manifests and the roots a session wrote`

### Task 3: Pairs, answers and groups

Tests (`crates/store/src/linking.rs` against an index with two git fixtures):
- `asks_each_proven_pair_once`: asserts these.
  - Three Team projects with proofs a–b and b–c give two pairs, a–b first.
  - After Yes to a–b and Yes to b–c, `groups` is `{a, b, c}`.
  - A later proof a–c is not asked, and is listed as also.
  - After No to a–b, a–b is not asked again. It is asked again once a `deploys` proof appears that was not in the No's proof list.
  - A pair whose proof file changed so that it no longer holds is not asked, and a Yes to it is `proof_no_longer_holds`.

- [ ] `feat(store): ask each proven pair once and group by the owner's yes`

### Task 4: The hub's methods

Tests (`crates/runtime/src/daemon/finding.rs` with the fake host):
- `links_through_the_hub`: asserts these.
  - `linking.pairs` answers the fake's pairs, and `linking.answer` writes through.
  - `linking.maybe` lists hint pairs.
  - `linking.groups` reflects the Yes answers.
  - Every method's schema holds.

- [ ] `feat(runtime): ask about links from the hub`

### Task 5: The screens

Tests (`pages/find/Together.test.tsx`):
- `asks_one_pair_at_a_time_with_its_proof`: asserts these.
  - With two pairs, the page shows "Question 1 of 2" and the sentence. ⓘ shows "tidepool-web/package.json, line 14:" and its text.
  - **Yes, one team** sends `linking.answer { answer: "yes" }` and shows "Question 2 of 2" with its "Also:" line.
  - **No, keep separate** sends `no`.
  - After the last pair, "That's all we could prove" lists the group, and "Maybe related (1)" opens to **Separate** and **One team**.

- [ ] `feat(web): do these work together?`

### Task 6: Spec

`docs/SPEC.md` revision 0.149 (2026-10-22), "records phase 10 step 06b, linking with proof". It covers:
- 3 and 8.4: migration 9, the files read at `HEAD` and their limits;
- F1 and 4.4: the proofs, hints, answers and the screen;
- the rule that no link is made without the owner's Yes to a proof that holds.

Then the project plan: row 06b becomes 06b (this) and 06e (the proposal, names, creating teams, undo, fixing a mistake, suggested connections).

- [ ] `docs(spec): linking with proof, revision 0.149`

## Verification

```
python3 reaper.py cargo xtask check
python3 reaper.py cargo test -p catervas-store --test linking -- --include-ignored
# expected: xtask check: ok; the linking tests pass
```
