# Phase 6, step 03: Component library

Status: ready
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 10 (WCAG 2.2 AA, 360 px, strings externalized, clients built from the brand's tokens) and 14
Depends on: step 01 (`@farik/brand`, done) and step 02 (done, 11e324f) of this phase
Readiness confirmed by: fresh-session reviewer, 2026-09-29 (one round, ready with findings, no decision open; its fourteen findings folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`@farik/ui` holds the parts every screen of the web app is built from. Each is a React component styled only by `@farik/brand`'s tokens, keyboard-usable, labelled for screen readers, and checked by `axe-core`. The parts are:
- buttons and form controls (text field, text area, choice cards, switch);
- a dialog and a stepper;
- a list, a table, a kanban column, and a chat list;
- a diff viewer;
- the avatar, the role tag, and the status word.

Every visible default string is in one file. The code-changes view gets two new pale tints, which the founder approved on 2026-09-28. A gallery page shows every part in light and dark and at phone width, and the step is done when the founder signs it off.

Out of scope: screens, routing, and data (steps 04 to 09), the task row's composition (step 08), and the office scene (phase 10).

## Decisions

- **Two new tokens** (the founder, 2026-09-28, option A of the diff-colours page): `diff-added` `#E3EBDF` light / `#243029` dark, and `diff-removed` `#F2DDD3` / `#3A2822`.
  - They are backgrounds under `ink` (14.83, 13.84, 11.23, and 11.41 to 1).
  - `TEXT_PAIRS` (one list, checked in each theme) gains `ink` on each, at a minimum of 4.5, going from 24 pairs to 26; the pinned-pairs test gains the two rows.
  - The one-colour-per-job test gains both.
  - `brand.md` records them with the date.
  - Diff lines keep their `+`/`−` marks, so colour is never alone.
- **Nested themes.** `tokens.css` puts light under `:root, [data-theme="light"]` and dark under `:root[data-theme="dark"], [data-theme="dark"]`, so an element can carry a theme of its own. The gallery needs this, and so will a dark panel inside a light page. It fixes step 01's finding that the brand sheet's dark column inherited light variables.
  - Because `color` and `background` computed on an ancestor are inherited as values, not re-resolved, the generated `[data-theme]` rules (not `:root`) also set `color: var(--farik-color-ink); background-color: var(--farik-color-page)`, so a themed element paints its own theme's text and ground.
  - The existing `generate.test.ts` tests that find blocks by the literal `:root {` and `:root[data-theme="dark"] {` are rewritten to find them by the new selector lists (they are rewritten, not added).
  - The project plan's step 01 interface line changes to these selectors in the same commit.
- **Styling:**
  - CSS Modules, one `<Component>.module.css` beside each component. Vite and Vitest support them natively, so no library is needed.
  - Every colour is a `var(--farik-…)` (sizes and spacing too, where a token exists; the 2 px focus outline, `overflow-x`, `white-space` and `font-variant-numeric` are literal properties). A test fails on any hex, `rgb(`, `hsl(`, or named colour in `packages/ui/src/**/*.css` (spec 10: no colour written by hand).
  - Rejected: a CSS-in-JS library, and Tailwind, which would duplicate the tokens.
- **Toolchain.** Everything is pinned exactly in `packages/ui/package.json`.
  - Dependencies: `react` and `react-dom` `19.3.0`; `@farik/brand` (`workspace:*`).
  - Dev dependencies:
    - `@types/react` and `@types/react-dom` `19.3.0`;
    - `@testing-library/react` `16.3.3`, `@testing-library/dom` `10.4.2`, `@testing-library/user-event` `14.6.7`;
    - `jsdom` `30.1.1`, `axe-core` `4.13.0`, and `vite` `8.3.1`, the versions step 01 uses.
  - `tsconfig.json` extends the base, sets `"jsx": "react-jsx"` and `"types": ["vite/client"]` (which types `*.module.css` and `*.png` imports), and includes `src` and `gallery`.
  - `vitest.config.ts`: `environment: "jsdom"`, `setupFiles: ["src/test/setup.ts"]`, `include: ["src/**/*.test.{ts,tsx}", "gallery/**/*.test.tsx"]`. `setup.ts` registers `afterEach(cleanup)` (Vitest has no globals, so Testing Library's automatic cleanup does not register); Task 4 adds its import of `./dialog-shim.ts`.
  - No `@vitejs/plugin-react`: Vite 8 and Vitest transform JSX without it (checked by the readiness reviewer on these versions, test and build), and fast refresh is not needed for a library.
- **Assets.** Avatars are imported as modules (`import pm from '@farik/brand/assets/avatars/product-manager-256.png'`) in `src/avatars.ts`, which exports `AVATAR_URLS: Record<AvatarKey, string>`. `AvatarKey` comes from `@farik/brand`'s `AVATAR_KEYS`. A role's colour and default avatar: `product_manager` → `role-product-manager`, `product-manager`; `scrum_master` → `role-scrum-master`, `scrum-master`; `architect` → `role-architect`, `architect`; `software_developer` → `role-developer`, `developer`; `marketing_specialist` → `role-marketing-specialist`, `marketing-specialist`.
- **Strings.** `src/strings.ts` exports `const uiStrings` with every string a component shows on its own: `close`, `noChanges`, `added`, `removed`, `stepOf(n, m)`, `required`, `busy`, `roleShort: Record<Role, string>` (`PM`, `SM`, `ARCH`, `DEV`, `MKT`) and `roleName: Record<Role, string>` (`Product Manager`, `Scrum Master`, `Architect`, `Software Developer`, `Marketing Specialist`). Callers pass every other string: labels, titles, and messages. English only (spec 10). A second language changes this one file.
- **Accessibility rules** (WCAG 2.2 AA, `web-ui.md`'s quality floor):
  - every control has a programmatic label;
  - errors are linked with `aria-describedby` and set `aria-invalid`;
  - focus is visible, a 2 px `--farik-color-focus` outline with a 2 px offset (a `:focus-visible` rule in each module);
  - motion stops under `prefers-reduced-motion`;
  - status is never shown by colour alone;
  - agent-written text is rendered as text, never as HTML.

  `src/test/axe.ts` exports `expectNoAxeViolations(container: Element): Promise<void>`, reached by other packages as `@farik/ui/test` (a separate `package.json` export, so `axe-core` never ships in an app bundle). It runs `axe-core` with its `color-contrast` rule off, because jsdom does not lay out, and the brand's contrast test holds colour.
- **The components and their props.** Every component is a named export, one per file (code.md).
  - `Button { kind?: 'primary' | 'secondary' | 'quiet' = 'secondary'; type?: 'button' | 'submit' = 'button'; disabled?; busy?; onClick?; children }`.
    - `primary` is `action` with `action-ink`: the one main action on a screen.
    - `quiet` is underlined ink, like a link.
    - `busy` sets `aria-busy` and `disabled` and appends `uiStrings.busy`.
  - `TextField { id; label; value; onChange(value: string); hint?; error?; type?: 'text' | 'password' | 'url' = 'text'; required? }`.
  - `TextArea` has the same props without `type`, and with `rows?: number = 4`.
  - `Choice<V extends string> { name; legend; options: { value: V; label; description? }[]; value: V; onChange(value: V) }`. It renders a `fieldset` of radio cards, as in the mockups' `SetupFinish`. The arrow keys move between options, as native radios do.
  - `Switch { id; label; checked; onChange(checked: boolean); description? }`. It is a `button` with `role="switch"` and `aria-checked`.
  - `Dialog { open; title; onClose(); children; actions? }`.
    - It is the native `<dialog>`, opened with `showModal()` when `open` becomes true, and labelled by its title.
    - Escape (the `cancel` event) and the close button call `onClose`.
    - Rejected: a hand-made focus trap, which the native modal already gives.
    - jsdom 30.1.1 has no `showModal` or `close` (checked), so `src/test/dialog-shim.ts` defines them on `HTMLDialogElement.prototype` when missing (setting and removing `open`), with a comment saying why. Playwright in step 04 tests the real one.
  - `Stepper { steps: string[]; current: number }`. It is an `ol`, with `aria-current="step"` on the current step and `uiStrings.stepOf(current + 1, steps.length)` for screen readers.
  - `List<T> { label; items: T[]; getKey(item: T): string; render(item: T): ReactNode; empty: ReactNode }`. It is a `ul` of rows divided by `rule`, with no card around it (`web-ui.md`, pass 2).
  - `Table<T> { caption; columns: { key; header; align?: 'start' | 'end'; render(row: T): ReactNode }[]; rows: T[]; getKey(row: T): string }`.
    - It is a `table` with a `caption` and `th scope="col"`, wrapped in its own `overflow-x: auto` box.
    - `end` columns use tabular numbers.
  - `KanbanColumn { id; title; count: number; children }`. It is a `section` whose `aria-labelledby` points at the title's own span, so its name is the title alone; the heading shows the title and then the count.
  - `ChatList { label; messages: { id; author: { name; role?: Role; avatarKey?: AvatarKey }; time: string; text: string; thread?: string }[] }`. It is an `ol`. Each message shows its avatar, name, role tag, time, and text, with line breaks kept.
  - `DiffView { diff: string; label }`.
    - It parses unified diff text with `parseDiff(diff: string): DiffFile[]` (`src/parse-diff.ts`), where `DiffFile = { path: string; lines: { kind: 'hunk' | 'context' | 'added' | 'removed'; text: string }[] }`. `path` drops the `a/` or `b/` prefix and comes from the `+++ b/…` line, or from `--- a/…` when the file was deleted (`+++ /dev/null`). A `\ No newline at end of file` line is dropped.
    - It shows each file under its path, lines in the mono face, and `added` and `removed` lines on their tints with the `+`/`−` mark and a visually hidden `uiStrings.added` or `removed`.
    - Empty or unparsable text shows `uiStrings.noChanges`.
    - A `ponytail:` comment marks that very long diffs render whole. Windowing comes if a real diff is slow.
  - `Avatar { avatarKey: AvatarKey; name: string; size?: 32 | 48 | 64 = 48 }`. It is an `img` with `alt={name}`, `image-rendering: pixelated`, and radius 0.
  - `RoleTag { role: Role }`.
    - `Role` is the wire's `'product_manager' | 'scrum_master' | 'architect' | 'software_developer' | 'marketing_specialist'` (`team.schema.json`'s `$defs.role`).
    - It renders `uiStrings.roleShort[role]` in an `abbr` whose `title` is `uiStrings.roleName[role]`, on the role's token with `role-ink`.
  - `StatusWord { tone: 'done' | 'working' | 'waiting'; children }`. The text is in the tone's `status-*` colour.
- **The gallery** is `packages/ui/gallery/index.html`, `gallery/Gallery.tsx` (exports `Gallery`), and `gallery/main.tsx` (mounts it). It shows every component with example content in a light column and a dark column (nested themes), and has a 360 px frame. Each column's `Dialog` starts closed, with a button that opens it (two open modals would make the page inert), and every `id` carries the column's suffix so they stay unique.
  - `pnpm --filter @farik/ui gallery` runs `vite build --config gallery/vite.config.ts` into `packages/ui/dist/gallery/` with `base: './'`.
  - It is published privately for the founder.
  - The step is `done` only after the founder signs it off, recorded in the status line.

## File map

```
packages/brand/tokens/tokens.json, src/generate.ts, src/contrast.ts   modifies: diff tints; nested themes; pairs (T1)
packages/brand/src/{generate,contrast}.test.ts, docs/brand/brand.md    tests / modifies (T1)
packages/ui/{package.json,tsconfig.json,vitest.config.ts}             creates (T2)
packages/ui/src/{strings.ts,avatars.ts,index.ts,test/axe.ts,test/setup.ts}  creates (T2)
packages/ui/src/test/dialog-shim.ts                                  creates (T4), with setup.ts's one import line added in T4
docs/plans/project-plan.md                                           modifies: step 01 and 03 interface lines (T1, T2)
packages/ui/src/{Button,StatusWord,RoleTag,Avatar}.tsx (+ .module.css, .test.tsx)       creates (T2)
packages/ui/src/tokens-only.test.ts                                   creates (T2)
packages/ui/src/{TextField,TextArea,Choice,Switch}.tsx (+ css, tests)                    creates (T3)
packages/ui/src/{Dialog,Stepper}.tsx (+ css, tests)                   creates (T4)
packages/ui/src/{List,Table,KanbanColumn,ChatList}.tsx (+ css, tests)                    creates (T5)
packages/ui/src/{parse-diff.ts,DiffView.tsx} (+ css, tests)           creates (T6)
packages/ui/gallery/{index.html,Gallery.tsx,main.tsx,vite.config.ts,Gallery.test.tsx}  creates (T7)
pnpm-lock.yaml                                                        modifies (T2)
```

`src/index.ts` gains each task's exports in that task.

## Interfaces

Consumes: `tokens.css`, `fonts.css`, `AVATAR_KEYS`, `TEXT_PAIRS`, and the asset paths from `@farik/brand` (step 01); the root `pnpm check` (step 01).

Produces: the components and props above, plus `uiStrings`, `parseDiff`, `DiffFile`, `AVATAR_URLS`, `AvatarKey`, and `Role`, all exported from `@farik/ui`'s `src/index.ts`; and `expectNoAxeViolations`, exported from `@farik/ui/test`.

## Tasks

Every component test also calls `expectNoAxeViolations` on what it rendered, so that is not repeated below.

### Task 1: Diff tints and nested themes in the brand

Also updates `brand.md` and the project plan's step 01 interface line (the selectors).

Tests (in `packages/brand`):
- `writes the dark theme for any element that asks for it` asserts that the CSS has a rule whose selector list includes `[data-theme="dark"]` without `:root`, with `--farik-color-page: #161616`, and one with `[data-theme="light"]` and `#F3E7D3`.
- `holds ink on the diff tints to AA`: `TEXT_PAIRS` has `ink` on `diff-added` and on `diff-removed` at 4.5. The pinned-pairs test lists 26, and both themes pass.
- `a themed element paints its own text and ground`: the `[data-theme="dark"]` rule sets `color: var(--farik-color-ink)` and `background-color: var(--farik-color-page)`.
- The two existing colour tests are rewritten to find the blocks by the new selector lists.
- `gives every job its own colour`, extended, asserts that `diff-added` and `diff-removed` are among the jobs and are distinct from each other and from every other job, in each theme.

- [x] `feat(brand): add the diff tints and let any element take a theme`

### Task 2: The package, the small parts, and the rules

Also updates the project plan's step 03 interface line to this plan's exports (`uiStrings` rather than `UiStrings`, and `parseDiff`, `DiffFile`, `AVATAR_URLS`, `AvatarKey`, `Role`, the diff tints, the nested themes).

Tests:
- `uses no colour but the tokens` asserts that no `.css` file under `packages/ui/src` contains `#` followed by hex digits, `rgb(`, `hsl(`, or a CSS named colour from a fixed list (`white`, `black`, `red`, `blue`, `green`, `gray`, `grey`) matched as a whole word not joined by `-` (`(?<![\w-])(white|black|red|blue|green|gray|grey)(?![\w-])`), so `white-space` passes.
- `Button`: `renders a primary button that does what it says` asserts that the role is button, the name is the children's text, and a click calls `onClick` once. `a busy button cannot be pressed twice` asserts `disabled`, `aria-busy="true"`, and that a click calls nothing.
- `StatusWord`: `says the status in words` asserts the text is present, and that each tone's class differs.
- `RoleTag`: `names the role in full for screen readers` (one test looping over the five roles) asserts, for each, the text `uiStrings.roleShort[role]` inside an `abbr` whose `title` is `uiStrings.roleName[role]` (for `software_developer`: `DEV`, `Software Developer`).
- `Avatar`: `shows the agent's character with its name` asserts an `img` whose `alt` is the name and whose `src` is `AVATAR_URLS[avatarKey]`, at the given size in `width` and `height`.

- [x] `feat(ui): add the package with buttons, status words, role tags and avatars`

### Task 3: Form controls

Tests:
- `TextField`: `labels the field and reports changes` asserts, with a small stateful wrapper in the test holding `value`, that `getByLabelText(label)` finds the input and that typing `abc` calls `onChange` with `a`, `ab`, and `abc`. `says what is wrong and marks the field invalid` asserts that with `error`, the input has `aria-invalid="true"` and an `aria-describedby` whose element holds the error text.
- `TextArea`: `labels the area and keeps line breaks` asserts, with the same stateful wrapper, that the label finds a `textarea` and that typing `a{Enter}b` gives `a\nb`.
- `Choice`: `picks an option with a click or the arrow keys` asserts that a click selects option 2 (calling `onChange` with its value), and that ArrowDown from option 2 selects option 3. `names the group` asserts a `group` role named by the legend.
- `Switch`: `switches on and off` asserts `role="switch"` named by its label, `aria-checked` following `checked`, and that Space calls `onChange(!checked)`.

- [x] `feat(ui): add text fields, text areas, choice cards and switches`

### Task 4: Dialog and stepper

Creates `src/test/dialog-shim.ts` and adds its import to `setup.ts`.

Tests:
- `Dialog`: `opens as a modal named by its title` asserts that with `open` true, `showModal` was called and the dialog's accessible name is the title. `closes on Escape and on the close button` asserts that a `cancel` event and a click on the button named `uiStrings.close` each call `onClose`.
- `Stepper`: `marks the current step` asserts three `listitem`s, `aria-current="step"` only on the one at `current`, and the text `Step 2 of 3` present for `current = 1`.

- [x] `feat(ui): add the dialog and the stepper`

### Task 5: Lists, tables, the kanban column, and the chat

Tests:
- `List`: `shows each item or the empty state` asserts one `listitem` per item in order, and the `empty` node with none.
- `Table`: `captions the table and heads each column` asserts that a `table` named by its caption has `columnheader`s with the headers in order, one row per item, and `end` cells with the numeric class.
- `KanbanColumn`: `names the lane and counts its tasks` asserts a `region` whose accessible name is exactly `<title>` and whose heading's text contains the count.
- `ChatList`: `shows who said what, and when` asserts per message the author's name, the time, the text, and the avatar's `alt`. `shows an agent's text as text` asserts that a message `<img src=x onerror=alert(1)>` renders as that literal text, with no `img` added.

- [x] `feat(ui): add lists, tables, the kanban column and the chat list`

### Task 6: The diff viewer

Tests:
- `parseDiff`:
  - `reads files, hunks and lines` asserts that a two-file `git diff` gives two `DiffFile`s with their paths without the `b/` prefix, the `@@` lines as `hunk`, and `+`, `-`, and space lines as `added`, `removed`, and `context` with the mark stripped.
  - `ignores the header lines` asserts that `diff --git`, `index`, `---`, and `+++` lines give no line entries.
  - `reads a new and a deleted file` asserts that `--- /dev/null` with `+++ b/new.txt` gives path `new.txt`, that `--- a/old.txt` with `+++ /dev/null` gives path `old.txt`, and that a `\ No newline at end of file` line gives no entry.
  - `gives nothing for text that is not a diff` asserts `[]`.
- `DiffView`:
  - `shows added and removed lines with their marks` asserts each added line's text begins `+`, each removed line's begins `−` (U+2212), and each carries its visually hidden word.
  - `says when there are no changes` asserts `uiStrings.noChanges` for `''`.

- [x] `feat(ui): add the diff viewer`

### Task 7: The gallery

Tests:
- `shows every component in both themes` (`gallery/Gallery.test.tsx`) asserts that the rendered `Gallery` has two elements with `data-theme` `light` and `dark`, each containing one example of each of the fifteen components, found by a `data-component="<Name>"` wrapper, and no open `dialog`.
- The gallery passes `expectNoAxeViolations`.

Then `pnpm --filter @farik/ui gallery` builds it, and the controller publishes it for the founder.

- [x] `feat(ui): add the component gallery for sign-off`

## Verification

```
cargo xtask check
# expected: the Rust tests unchanged; pnpm: @farik/brand "Tests  30 passed (30)" (27 plus Task 1's three new tests;
#   the job test is extended and two colour tests rewritten, not added), @farik/protocol-client "Tests  6 passed (6)",
#   @farik/ui "Tests  28 passed (28)" (T2 6, T3 6, T4 3, T5 5, T6 6, T7 2); last line: xtask check: ok
pnpm --filter @farik/ui gallery   # writes packages/ui/dist/gallery/index.html, exit 0
```

Then the founder's sign-off of the gallery, recorded here as `Status: done (signed off by the founder, <date>)`.
