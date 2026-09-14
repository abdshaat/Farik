## Plan

Link to the step plan under `docs/plans/phase-<n>-<name>/`, or the reason this change has no plan (typo, one-line doc fix).

## Spec reference

`docs/SPEC.md` section and F-number this serves. If the spec changed, say what changed and why.

## What changed

Two to six sentences. Why, more than what; the diff shows what.

## Verification evidence

The exact command(s) run on the final commit and their output. Not a summary of the output.

```
pnpm check
<paste output>
```

If no check command exists yet, say so here.

## Decisions

ADRs added or changed, or "none".

## Checklist

- [ ] Every task in the plan is ticked and has a commit
- [ ] New behavior has tests that were watched to fail first
- [ ] No files changed outside the plan's file map, or the plan was updated and the reason is above
- [ ] `docs/SPEC.md` updated if behavior changed
- [ ] Commit messages and branch follow `docs/standards/code.md`
- [ ] I have not approved my own pull request
