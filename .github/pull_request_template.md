## Phase and plans

The phase this pull request delivers and links to every step plan under `docs/plans/phase-<n>-<name>/`, or the reason this change is outside a phase and has no plan (typo, one-line doc fix).

## Spec reference

`docs/SPEC.md` section and F-number this serves. If the spec changed, say what changed and why.

## What changed

Two to six sentences. Why, more than what; the diff shows what.

## Verification evidence

The exact command(s) run on the final commit and their output. Not a summary of the output.

```
cargo xtask check
<paste output>
```

If no check command exists yet, say so here.

## Decisions

ADRs added or changed, or "none".

## Checklist

- [ ] Every task in every step plan of the phase is ticked by its commit, and its tests were watched to fail first
- [ ] Each step landed with the full check passing locally and the `check` workflow passing on GitHub (ADR 0060: no agent review of code)
- [ ] No files changed outside the plan's file map, or the plan was updated and the reason is above
- [ ] `docs/SPEC.md` and any other document describing changed behavior updated; an ADR for any decision a future contributor would want the reason for
- [ ] Commit messages and branch follow `docs/standards/code.md` (the commit-msg hook is local; CI does not check them)
