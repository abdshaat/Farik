# Standalone: agent page polish

Status: draft
Branch: `fix/agent-page-polish` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` section 6.7 (the "Service logos and info buttons" paragraph, 0.79), F9 (the agent page)
Depends on: `docs/plans/standalone/agent-page-logos.md` (merged in #27)
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

## Goal

The founder looked at the agent page after #27 and approved three fixes on 2026-10-09. The Playwright logo sits beside the word "Playwright" inside the switch's label, not to the left of the switch. Each skill's description moves behind an ⓘ beside the skill's name, so the Skills section reads as names and buttons. And on a phone each kit row keeps its short line and its ⓘ together, so the ⓘ never wraps onto a line of its own. Out of scope: any other section, any copy change.

## Decisions

- `Switch` gains `icon?: ReactNode`, rendered inside the label span (`${id}-label`) before the label's text. An `<img alt="">` there adds nothing to the switch's accessible name, which stays exactly `label`. Rejected: a wrapper outside the switch (today's layout, which puts the logo left of the track).
- A skill row's description goes into an `InfoTip` placed right after the skill's name in the row head, id `skill-${r.level}-${r.name}-info` (a name is unique within its level, and the same name can be at two levels). A missing skill's line, "to review" and "replaced" stay visible: they are statuses.
- The kit row's `about` and its `InfoTip` sit in one inline span (`styles.muted`), the InfoTip after the text, so the ⓘ follows the last word wherever the line breaks. `.titled` keeps its flex wrap for logo, title and that span.
- The spec paragraph's list of what sits behind ⓘ adds "each skill's description".

## File map

```
packages/ui/src/Switch.tsx                      modifies: optional `icon`
packages/ui/src/Switch.test.tsx                 tests:    the `icon` prop
apps/web/src/pages/AgentEdit.tsx                modifies: Playwright's icon (Task 2), skill rows (Task 3), kit row span (Task 4)
apps/web/src/pages/connectors.test.tsx          tests:    Playwright's logo (Task 2), the kit row's ⓘ beside its line (Task 4)
apps/web/src/pages/skills.test.tsx              tests:    skill descriptions behind ⓘ (Task 3)
docs/SPEC.md                                    modifies: 6.7 paragraph (Task 3)
```

## Interfaces

Consumes: `Switch`, `InfoTip` (`@farik/ui`, on main); `ServiceLogo` (`apps/web/src/pages/ServiceLogo.tsx`, on main); `onlyInTips`-style helpers in `connectors.test.tsx` and `team.test.tsx` (on main).

Produces:

```ts
Switch: props & { icon?: ReactNode }   // inside `${id}-label`, before the label text
```

## Tasks

### Task 1: `icon` on Switch

Files: modified `packages/ui/src/Switch.tsx`, `Switch.test.tsx`

Tests:

- `it('shows an icon inside its label, before the words')` — with `icon={<img alt="" src="x.svg" />}`, the element with id `${id}-label` contains the `img` as its first element child and the label text after it; the switch's accessible name is exactly `label`.

- [ ] `feat(ui): let a switch carry an icon in its label`

### Task 2: Playwright's logo beside its name

Files: modified `AgentEdit.tsx` (the Playwright switch: drop the wrapping `div.titled` and the `ServiceLogo` before it; pass `icon={<ServiceLogo name="playwright" />}`), `connectors.test.tsx`
Consumes: Task 1

Tests:

- `it('shows Playwright's logo inside its label')` — the element `#connector-playwright-label` contains an `img` whose `getAttribute("src")` equals `serviceLogo("playwright")`, and the `switch` named "Playwright" is still found by that exact name. Replaces the logo assertion of `'shows Playwright with its logo, a short line and the rest behind info'`, whose other assertions stay.

- [ ] `fix(web): put Playwright's logo beside its name`

### Task 3: skill descriptions behind ⓘ

Files: modified `AgentEdit.tsx` (`SkillsSection` rows), `skills.test.tsx`, `docs/SPEC.md` 6.7
Consumes: `InfoTip`

Tests:

- `it('puts each skill's description behind an info button')` — for a role skill and a team skill: the description's text is inside the row's `role="tooltip"` element and in no other element of the row outside it; the row head holds the skill's name and the button "More about this".
- The existing assertions that a description is in the row (`inRow(...).textContent` containing it, skills.test.tsx:142 and its neighbours) stay as they are: tooltip text is in the row's text.

- [ ] `fix(web): hide each skill's description behind an info button`

### Task 4: the kit row's ⓘ stays with its line

Files: modified `AgentEdit.tsx` (`KitRow`'s head), `connectors.test.tsx`

Tests:

- `it('keeps a kit service's info button with its short line')` — in the Notion kit row, the element holding `about`'s text also contains the "More about this" button (`closest` span of the text contains the button).

- [ ] `fix(web): keep a kit row's info button beside its line on a phone`

## Verification

```
pnpm check
# expected: exit 0, every package's tests passed
cargo xtask check --integration
# expected: xtask check: ok (the e2e narrow() checks included)
```

Then the founder opens a Product Manager's and a Developer's agent page at desktop and phone width.
