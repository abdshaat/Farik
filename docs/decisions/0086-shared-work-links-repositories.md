# 0086. Shared work links repositories into one team

Date: 2026-10-23
Status: accepted (the founder, 2026-10-23, quoted below)

Amends ADR 0059's linking rule (`docs/design/many-teams.md` section 4) and phase 10 step 06b.

## Context

Phase 10 step 06b links two found projects only on a proof in their committed files: a path dependency, a submodule, a deploy file and so on. A session of Claude Code that wrote files in both was a hint only. It was shown under "Maybe related" and never enough to link the pair on its own.

The founder, 2026-10-23: "Sessions that work across repositories are to be multi repo teams. It should depend mainly on relationships between tasks." Asked three questions, the founder answered:

- **Ask first?** "Propose, owner confirms". Such a pair is proposed with its sessions as the reason, and the owner still confirms it.
- **The threshold.** "2+ sessions wrote in both". At least two separate sessions changed files in both repositories; one stray edit is not enough.
- **File proofs.** "Keep as supporting proof". Shared work leads; a file proof still proposes a pair with no shared work, and shows as "Also" beside it.

## Decision

1. **Shared work is a proof, and the first kind.** `shared_work` holds for two projects when at least two distinct Claude Code sessions each wrote a file under both of their roots. "Wrote" means a `Write`, `Edit` or `MultiEdit` tool call, as kept by step 06b in `session_roots_written`. A session's place is its session id, so a resumed session counts once. Reading a file never counts.
2. **The sentence and its evidence.** The sentence is "You worked on a and b together in N sessions." Behind ⓘ the screen lists up to three of those sessions by title and date, newest first, and "and N more". No conversation text is read or shown; titles are what step 02 keeps.
3. **The owner still confirms.** The pair is asked on "Do these work together?" like any proof. For a `shared_work` pair the screen leads with "These look like one team", and **Yes, one team** is the first, focused button. Nothing is linked until it is pressed. A No is kept, and the pair is not asked again unless a file proof of a kind not seen before appears.
4. **Shared work is asked first.** Pairs with `shared_work` come before pairs with only file proofs. A pair with both shows `shared_work` and lists the file proofs under "Also".
5. **One shared session stays a hint.** A pair with exactly one session that wrote in both keeps the `wrote_both` hint under "Maybe related", as before.
6. **Checked again.** When the pair is shown, and when the team is created, the number of shared sessions is counted again from the index. Below two, the proof is dropped, as a file proof that no longer holds is.
7. **The tempting corpus grows.** Two cases must give no proof: one session that wrote in both, and sessions that read both and wrote one. Two sessions that wrote in both must give `shared_work`.
8. **Catervas's own sessions do not count.** They are placed `catervas` by step 02 and are not the owner's work.

## Consequences

- Repositories the owner works on together are proposed as one team even when nothing in their files points at the other. A front end and an API that talk over HTTP are one example.
- A repository the owner happened to fix twice while working elsewhere is proposed too. Separate stays one click away, and a No is kept.
- The proof needs `session_roots_written`, which step 06b fills from the first scan after its migration. Nothing new is read.
