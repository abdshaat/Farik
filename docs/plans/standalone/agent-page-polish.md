# Standalone: agent page polish

Status: ready
Branch: `fix/agent-page-polish` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` section 6.7 (the "Service logos and info buttons" paragraph, 0.79), F9 (the agent page)
Depends on: `docs/plans/standalone/agent-page-logos.md` (merged in #27)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-09 (one round, against `docs/standards/workflow.md` stage 2)

## Goal

The founder looked at the agent page after #27 and approved three fixes on 2026-10-09. The Playwright logo sits beside the word "Playwright" inside the switch's label, not to the left of the switch. Each skill's description moves behind an ⓘ beside the skill's name, so the Skills section reads as names and buttons. And on a phone each kit row keeps its short line and its ⓘ together, so the ⓘ never wraps onto a line of its own. Out of scope: any other section, any copy change.

## Decisions

- `Switch` gains `icon?: ReactNode`, rendered inside the label span (`${id}-label`) before the label's text. An `<img alt="">` there adds nothing to the switch's accessible name, which stays exactly `label`. `.label` (`Switch.module.css`) becomes `display: inline-flex; align-items: center; gap: var(--farik-space-1)` so the icon does not touch the word. Rejected: a wrapper outside the switch (today's layout, which puts the logo left of the track).
- A skill row's description goes into an `InfoTip` inside the name's existing `<span>` in the row head, after `<strong>{r.name}</strong>`, holding `visibly(r.description)`, id `skill-${r.level}-${r.name}-info` (a name is unique within its level, and the same name can be at two levels); the `<p>{visibly(r.description)}</p>` is removed. (`.rowHead` is `space-between`, so a sibling would land mid-row.) A missing row has no InfoTip. A missing skill's line, "to review" and "replaced" stay visible: they are statuses.
- The kit row's `about` and its `InfoTip` sit in one inline span (`styles.muted`), the InfoTip after the text, so the ⓘ follows the last word wherever the line breaks. `.titled` keeps its flex wrap for logo, title and that span.
- Spec revision 0.80 (2026-10-09), a sentence in the header at `docs/SPEC.md:3` as every revision has: each skill's description sits behind an info button, and Playwright's logo sits inside its switch's label. The 6.7 paragraph adds "each skill's description" to what sits behind ⓘ and is marked "changed in 0.80".

## File map

```
packages/ui/src/Switch.tsx, Switch.module.css   modifies: optional `icon`; `.label` spacing
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

Files: modified `packages/ui/src/Switch.tsx`, `Switch.module.css`, `Switch.test.tsx`

Tests:

- `it('shows an icon inside its label, before the words')` — with `icon={<img alt="" src="x.svg" />}`, the element with id `${id}-label` contains the `img` as its first element child and the label text after it; the switch's accessible name is exactly `label`.

- [x] `feat(ui): let a switch carry an icon in its label`

### Task 2: Playwright's logo beside its name

Files: modified `AgentEdit.tsx` (the Playwright switch: drop the wrapping `div.titled` and the `ServiceLogo` before it; pass `icon={<ServiceLogo name="playwright" />}`), `connectors.test.tsx`
Consumes: Task 1

Tests:

- `it('shows Playwright's logo inside its label')` — the element `#connector-playwright-label` contains an `img` whose `getAttribute("src")` equals `serviceLogo("playwright")`, and the `switch` named "Playwright" is still found by that exact name. Replaces the logo assertion of `'shows Playwright with its logo, a short line and the rest behind info'`, whose other assertions stay.

- [ ] `fix(web): put Playwright's logo beside its name`

### Task 3: skill descriptions behind ⓘ

Files: modified `AgentEdit.tsx` (`SkillsSection` rows), `skills.test.tsx`, `docs/SPEC.md` (header and 6.7)
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
