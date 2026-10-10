# Standalone: Farik's mark in the rail

Status: ready
Branch: `feat/rail-logo` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` section 14 (brand); no spec text describes the rail's name line, so none changes
Depends on: main at 571b15f3
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-09 (one round, against `docs/standards/workflow.md` stage 2)

## Goal

The founder asked on 2026-10-09 for Farik's logo at the top left, next to the word "Farik". Done means: on a wide screen the rail's first line shows Farik's mark at 24 px left of the "FARIK" word, on one line. Out of scope: the phone's top bar (it shows the project's name, not the word), favicons, other pages.

## Decisions

- The image is `@farik/brand/assets/icons/icon-48.png` shown at 24×24: the same mark as `logo-mark-1254.png` (used by the wizard and the ads row) at 4 KB instead of 687 KB, and 48 px stays sharp at 2× density. Rejected: `logo-mark-1254.png` (687 KB for a 24 px picture).
- `<img alt="" width="24" height="24">`: decorative, because the word beside it names it; the line's text stays exactly `t("brand")`.
- `.logo` becomes `display: flex; align-items: center; gap: var(--farik-space-2)`; its font, spacing and case stay.

## File map

```
apps/web/src/shell/Shell.tsx          modifies: the mark in the rail's name line
apps/web/src/shell/Shell.module.css   modifies: `.logo` lays the mark and the word on one line
apps/web/src/shell/Shell.test.tsx     tests:    the mark
```

## Interfaces

Consumes: `@farik/brand/assets/*` export (`packages/brand/package.json`, on main); `t("brand")`.

Produces: nothing.

## Tasks

### Task 1: the mark beside the word

Files: modified `Shell.tsx`, `Shell.module.css`, tested by `Shell.test.tsx`

Tests (the test calls `media.set(WIDE, true)`, `renderApp("/")`, `answerStatus(socket, false)`, then finds the line as `within(getByRole("banner")).getByText(en.brand)`, as the file's other tests set up):

- `it("shows Farik's mark beside its name in the rail")` — that line contains an `img` whose `getAttribute("src")` equals the imported `icon-48.png` URL, whose `alt` is `""`, and whose `width` and `height` are `"24"`; the line's `textContent` is exactly `en.brand`.

- [ ] `feat(web): show Farik's mark beside its name in the rail`

## Verification

```
pnpm check
# expected: exit 0, every package's tests passed
cargo xtask check --integration
# expected: xtask check: ok
```

Then the founder opens Farik on a wide screen and sees the mark beside the word.
