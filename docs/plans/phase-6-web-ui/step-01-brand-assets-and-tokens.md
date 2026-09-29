# Phase 6, step 01: Brand assets and tokens

Status: done (signed off by the founder, 2026-09-28, on the brand sheet with the muted palette)
Branch: `phase/6-web-ui`
Spec: `docs/SPEC.md` sections 10 (WCAG 2.2 AA, clients built from the brand's tokens, offline) and 14 (brand)
Depends on: phase 5 (merged in #16: `docs/brand/brand.md`, `docs/design/web-ui.md`, the founder's files in `docs/brand/assets/`); the planning commit of this phase (111629a)
Readiness confirmed by: fresh-session reviewer, 2026-09-28 (one round, ready with findings, no decision open; its seventeen findings folded in)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The repository has a front end toolchain and the brand as code. `pnpm check` runs typecheck, Biome, and Vitest over a pnpm workspace, and `cargo xtask check` runs it after the Rust checks, locally and in CI. `@farik/brand` holds the design tokens as one JSON source that generates a CSS custom-property sheet and a TypeScript module for the light and dark themes. It also holds the three fonts, the logo, the wordmark and icons derived from the founder's PNGs, and the ten avatars. A test holds every text and control colour pair to WCAG 2.2 AA in both themes. A brand sheet page renders all of it for the founder's sign-off. Out of scope: any component (step 03), any app (step 04), the desktop icon formats `.ico` and `.icns` (phase 10 packs them from the 1024 px master with Tauri's tooling), and an SVG logo (the founder, 2026-09-28: PNG now, SVG later if a vector file is supplied).

## Decisions

- The toolchain is ADR 0002's. Versions are exact, pinned in `package.json`, and the lockfile is committed:
  - pnpm `12.3.4`, set in the root `packageManager`;
  - Node `24.14.0`, set in `.node-version`, which CI's `actions/setup-node` reads;
  - in the root `package.json`: TypeScript `7.0.2`, `@biomejs/biome` `2.5.14`, Vitest `5.0.2`;
  - in `packages/brand/package.json`: Vite `8.3.1`, `sharp` `0.35.5`, `jsdom` `30.1.1`, `axe-core` `4.13.0` (dev dependencies), and `@fontsource/space-grotesk`, `@fontsource/jetbrains-mono`, `@fontsource/silkscreen`, each `5.3.0` (dependencies);
  - CI actions `pnpm/action-setup@v4` and `actions/setup-node@v5`.

  Rejected: ranges. Every-phase rule. Rejected from ADR 0002's list for now: lefthook, because `cargo xtask install-hooks` already writes the hooks, and Changesets, because nothing is published before phase 9.
- `tsconfig.base.json` sets `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `module`/`moduleResolution` `nodenext`, `target` `es2024`, `verbatimModuleSyntax` (code.md), and, so that Node runs the sources directly, `noEmit`, `allowImportingTsExtensions`, and `erasableSyntaxOnly`, and `skipLibCheck` (dependency `.d.ts` files are not ours to typecheck); imports between source files use the `.ts` extension. Every package is `"type": "module"`. One `biome.json` at the root, with the recommended rules and the formatter on. Biome ignores generated files and `docs/`.
- The root `pnpm check` runs four things in order: `pnpm -r --if-present generate`, then `pnpm -r --if-present typecheck`, then `biome check .`, then `pnpm -r --if-present test`. Each package's `test` is `vitest run`. Rejected: a task runner (turbo). Four commands need none.
- `cargo xtask check` runs `pnpm install --frozen-lockfile`, then `pnpm check`, after `core-io`, when the workspace root has a `package.json`. It prints nothing extra, and a failing exit code fails the check. The decision is a pure function (`xtask::check::front_end_commands`), so a test can hold it; `main.rs` only runs what it returns. `pre-commit` does not run pnpm, because it stays fast.
- CI's one job gains `pnpm/action-setup` (reading `packageManager`) and `actions/setup-node` (reading `.node-version`, with the pnpm cache) before `cargo xtask check --integration`.
- Generated files are not committed (code.md, "Nothing generated is committed"). `packages/brand/src/generated/` and `packages/brand/dist/` are gitignored, and `generate` makes them. Rejected: committing them with a staleness check, which is a second mechanism for what running the generator already gives.
- The token source is `packages/brand/tokens/tokens.json`, with this shape:

  ```
  { "color": { "light": {...}, "dark": {...} }, "type": {...}, "space": {...}, "radius": {...} }
  ```

  - **Colour tokens**, in both themes: `page`, `surface`, `ink`, `ink-muted`, `rule`, `control-border`, `band`, `band-ink`, `action`, `action-ink`, `link`, `focus`, `status-done`, `status-working`, `status-waiting`, and `role-product-manager`, `role-scrum-master`, `role-architect`, `role-developer`, `role-marketing-specialist`, `role-ink`.
  - **Light values** come from `docs/design/web-ui.md` and `brand.md`. `link` is Midnight, `status-waiting` is Clay Text `#96533A`, `status-done` is Moss Text `#536C59`, `status-working` is Slate Text `#44607F`, `control-border` is `#8A7F6E`, `band-ink` is Soft Sand, and `role-ink` is Midnight.
  - **Every value**, light then dark: `page` `#F3E7D3` / `#161616`; `surface` `#FBF6EC` / `#1F1F1F`; `ink` `#161616` / `#F3E7D3`; `ink-muted` `#5B5347` / `#B9AE9C`; `rule` `#D9CBB3` / `#333333`; `control-border` `#8A7F6E` / `#8A7F6E`; `band` `#161616` / `#0E0E0E`; `band-ink` `#F3E7D3` / `#F3E7D3`; `action` `#D8896A` / `#D8896A`; `action-ink` `#161616` / `#161616`; `link` `#161616` / `#F3E7D3` (underlined ink); `focus` `#5F7A9B` / `#A7BCD6`; `status-done` `#536C59` / `#6E8F76`; `status-working` `#44607F` / `#8AA3C2`; `status-waiting` `#96533A` / `#E0A68C`; `role-product-manager` `#C9A0A6`, `role-scrum-master` `#A3B8A0`, `role-architect` `#D6B77A`, `role-developer` `#B7C7DA`, `role-marketing-specialist` `#B3A9CF` (both themes), and `role-ink` `#161616`, the same in both themes. Changed 2026-09-28 by the founder after the first brand sheet: muted colours, one per job. Hex digits are upper case in the source and in every output. `web-ui.md`'s `role-*` row, which predates the Architect's Amber, is corrected to these five in Task 2.
  - **`focus`** is Slate Focus `#5F7A9B` in light and Mist `#A7BCD6` in dark (changed 2026-09-28 by the founder after the first brand sheet; the light ring was too blue). Slate Focus is 3.62:1 on Soft Sand, over 1.4.11's 3:1.
  - **`rule` is decorative** (dividers), so it is not held to a ratio. A control's edge uses `control-border`, which is.
  - **Type steps** are `display`, `title`, `heading`, `body`, `small`, and `code`. Each has a `size` and a `line_height` in px, a `weight` (400 or 600), and a `family` (`pixel`, `sans`, or `mono`), from `web-ui.md`'s table: title and heading 600, the rest 400. A family is written in CSS as its stack: `pixel` `'Silkscreen', monospace`; `sans` `'Space Grotesk', system-ui, sans-serif`; `mono` `'JetBrains Mono', ui-monospace, monospace`.
  - **`space`** is `0, 4, 8, 12, 16, 24, 32, 48, 64` under keys `0` to `8` (`--farik-space-0` to `--farik-space-8`; in TypeScript `tokens.space[0]` to `tokens.space[8]`).
  - **`radius`** is `none` 0, `control` 4, and `raised` 8.
  - **No elevation tokens:** `web-ui.md`, pass 2, has no drop shadows. Rejected: an unused elevation scale (the old phase 5 bullet), which was YAGNI before the design removed shadows.
- The generator is `packages/brand/src/generate.ts`, run with `node` directly: Node 24 strips types, so no `tsx` is needed. It exports `generateCss(tokens: Tokens): string` and `generateTs(tokens: Tokens): string` and writes two files:
  - `dist/tokens.css`: light under `:root` and dark under `:root[data-theme="dark"]`. Custom properties are named `--farik-<group>-<name>`, for example `--farik-color-ink`, and a type step's four as `--farik-type-<step>-size`, `-line-height`, `-weight`, and `-family`; sizes carry `px`. Choosing the theme (and "match my computer") is the web shell's job (step 04), which sets `data-theme`.
  - `src/generated/tokens.ts`: `export const tokens`, `as const`, with keys in `camelCase` (rule 6), and `export type ThemeName = 'light' | 'dark'`.

  A colour token present in one theme and missing from the other is an error that names it, so a theme can never be half-designed. The two files are written only when the module is run (`import.meta.main`), so tests import the functions without writing anything.
- The contrast check is our own code, the WCAG 2.x relative-luminance formula (`src/contrast.ts`), because it is ten lines. Rejected: a colour library. `TEXT_PAIRS` lists each pair by token name with its minimum:
  - 4.5 for text: `ink`, `ink-muted`, `link`, and each `status-*` on `page` and on `surface`; `action-ink` on `action`; `band-ink` on `band`; `role-ink` on each `role-*`;
  - 3 for controls: `control-border` and `focus` on `page` and on `surface`, and `focus` on `band`, which holds the rail's focusable places.

  The test runs every pair in both themes.
- **Fonts** come from the `@fontsource` packages. `src/fonts.css` imports weights 400 and 600 of Space Grotesk, 400 of JetBrains Mono, and 400 of Silkscreen. A bundler copies the `woff2` files, so nothing is fetched from a font service at run time (spec 10). Rejected: copying the font files into the repository, which the packages already ship under the OFL.
- **Images.** The founder's originals stay in `docs/brand/assets/`. What the apps import is under `packages/brand/assets/`, named per code.md's asset row:
  - `logo-mark-1254.png` and `wordmark-1024.png`, copied unchanged;
  - `icons/icon-<n>.png` for `n` in `16, 32, 48, 180, 192, 512, 1024`: favicon, Apple touch, web manifest, and the desktop master;
  - `avatars/<key>-256.png` for the five role keys (`product-manager`, `scrum-master`, `architect`, `developer`, `marketing-specialist`) and `extra-1` to `extra-5`.

  The avatars are moved there with `git mv` from `docs/brand/assets/avatars/`, not regenerated, and `brand.md` is updated where it names their path (the Files section and the Characters section) and where it says this step regenerates them. The icons are derived once by `src/icons.ts` (`sharp`, a Lanczos resize of `logo-mark.png` as it is: the file already draws the Midnight rounded tile, with transparent corners, so nothing is composed) and committed as binary assets. `pnpm --filter @farik/brand icons` re-derives them when the master changes, and `check` does not run it, because `sharp` is slow and the output is fixed.
- `package.json` `exports`: `"."` is `src/index.ts`, which exports `tokens`, `ThemeName`, `contrastRatio`, `TEXT_PAIRS`, `AVATAR_KEYS`, and `ICON_SIZES`. There are also `"./tokens.css"`, `"./fonts.css"`, and `"./assets/*"`. The package is private and consumed as TypeScript source by Vite and Vitest, so it has no build step of its own.
- **The brand sheet** is `packages/brand/sheet/index.html` with `sheet/sheet.ts`, which exports `renderSheet(root: HTMLElement): void`. It shows:
  - both themes side by side: every colour swatch with its name and hex, every `TEXT_PAIRS` pair with its measured ratio and minimum, and the type scale in its faces;
  - the logo and wordmark lockups on dark and light, every icon size, and the ten avatars.

  Each swatch is an element with `data-swatch="<theme>:<name>"` and each pair row `data-pair="<theme>:<foreground>/<background>"`. `pnpm --filter @farik/brand sheet` runs `vite build --config sheet/vite.config.ts` into `packages/brand/dist/sheet/`; that config is the sheet's alone, so Vitest, which reads `packages/brand/vitest.config.ts` (tests under `src/` and `sheet/`, environment `node`), is unaffected. `sheet/sheet.test.ts` opts into jsdom with `// @vitest-environment jsdom`. The founder signs it off from a private artifact of that build, and the step is `done` only after that sign-off, recorded in this file's status line.

## File map

```
package.json, pnpm-workspace.yaml, pnpm-lock.yaml     creates: the workspace root, pinned tools, the check script
.node-version, tsconfig.base.json, biome.json          creates: Node pin, shared compiler and lint settings
.gitignore                                             modifies: packages/*/src/generated/
xtask/src/check.rs                                     modifies: front_end_commands; tested in mod tests
xtask/src/main.rs                                      modifies: check runs front_end_commands after core_io
.github/workflows/check.yml                            modifies: pnpm and Node setup
packages/brand/package.json, tsconfig.json             creates: the package, scripts generate/typecheck/test/icons/sheet
packages/brand/tokens/tokens.json                      creates: the one token source
packages/brand/src/tokens-schema.ts                    creates: the Tokens type and readTokens(json: unknown): Tokens
packages/brand/src/generate.ts, generate.test.ts       creates: CSS and TS generation, and its tests
packages/brand/src/contrast.ts, contrast.test.ts       creates: contrastRatio, TEXT_PAIRS, and the AA test
packages/brand/src/fonts.css                           creates: the three families
packages/brand/src/icons.ts                            creates: the icon derivation script
packages/brand/src/assets.ts, assets.test.ts           creates: AVATAR_KEYS, ICON_SIZES, and the asset test
packages/brand/src/index.ts                            creates: the package's exports
packages/brand/assets/**                               creates: logo, wordmark, icons; moves: avatars from docs/brand/assets/avatars/
packages/brand/sheet/index.html, sheet.ts, sheet.test.ts   creates: the brand sheet and its test
docs/brand/brand.md                                    modifies: where the avatars and icons live (Task 4)
docs/design/web-ui.md                                  modifies: the focus ring's light value and the role-* row (Task 2)
docs/plans/project-plan.md                             modifies: phase 6 step 01's interface line, to this plan's (Task 2)
packages/brand/vitest.config.ts, sheet/vite.config.ts  creates: the tests' config (Task 2); the sheet's build config (Task 5)
```

## Interfaces

Consumes: nothing from earlier steps. The founder's PNGs in `docs/brand/assets/`, and the colours and type in `brand.md` and `web-ui.md`.

Produces (TypeScript unless marked):

```ts
// xtask (Rust): pub fn front_end_commands(has_package_json: bool) -> Vec<Vec<&'static str>>
export type Tokens = { color: Record<ThemeName, Record<string, string>>; type: Record<string, TypeStep>;
  space: Record<string, number>; radius: Record<string, number> };
export type TypeStep = { size: number; lineHeight: number; weight: 400 | 600; family: 'pixel' | 'sans' | 'mono' };
export function readTokens(json: unknown): Tokens;          // throws Error naming the first bad or one-sided token
export function generateCss(tokens: Tokens): string;
export function generateTs(tokens: Tokens): string;
export function contrastRatio(foreground: string, background: string): number;  // hex #rrggbb, 1..21
export type TextPair = { foreground: string; background: string; minimum: 4.5 | 3 };
export const TEXT_PAIRS: readonly TextPair[];
export const AVATAR_KEYS: readonly string[];                // the ten keys, roles first
export const ICON_SIZES: readonly number[];                 // 16, 32, 48, 180, 192, 512, 1024
export function renderSheet(root: HTMLElement): void;       // sheet/sheet.ts
```

## Tasks

### Task 1: The workspace and the check

Files: created `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `.node-version`, `tsconfig.base.json`, `biome.json`; modified `.gitignore`, `xtask/src/check.rs`, `xtask/src/main.rs`, `.github/workflows/check.yml`; tested by `xtask/src/check.rs`'s `mod tests`
Produces: `front_end_commands`, and `pnpm check` over an empty workspace
Consumes: nothing

Tests:
- `runs_pnpm_install_and_check_when_the_workspace_has_a_package_json`: asserts that `front_end_commands(true)` is exactly `[["install", "--frozen-lockfile"], ["check"]]`, in that order.
- `runs_no_front_end_command_without_a_package_json`: asserts that `front_end_commands(false)` is empty, so a checkout without the front end, or an older one, checks as before.

By hand before the commit: `pnpm check` in the empty workspace exits 0 (Biome checks only the root JSON files).

- [x] `build(repo): add the pnpm workspace and run pnpm check from xtask`

### Task 2: Tokens and their generation

Files: created `packages/brand/{package.json,tsconfig.json,vitest.config.ts}`, `tokens/tokens.json`, `src/tokens-schema.ts`, `src/generate.ts`; modified `docs/design/web-ui.md` (focus ring, `role-*` row) and `docs/plans/project-plan.md` (step 01's interface line: no elevation, no committed generated files, PNG logo and wordmark, `<key>-256.png` avatars, fonts from `@fontsource`); tested by `src/generate.test.ts`
Produces: `Tokens`, `TypeStep`, `readTokens`, `generateCss`, `generateTs`, and the `generate` script
Consumes: `pnpm check` from Task 1

Tests:
- `writes every colour token of the light theme under :root` asserts that the output of `generateCss(readTokens(tokens.json))` has one `--farik-color-<name>: <hex>;` inside `:root { … }` for each light colour, and `--farik-color-page: #F3E7D3`.
- `writes every colour token of the dark theme under :root[data-theme="dark"]` asserts the same for dark, including `--farik-color-page: #161616`.
- `writes each type step as size, line height, weight and family` asserts that `--farik-type-body-size: 16px;`, `--farik-type-body-line-height: 24px;`, `--farik-type-body-weight: 400;`, `--farik-type-title-weight: 600;`, and `--farik-type-body-family: 'Space Grotesk', system-ui, sans-serif;` are present.
- `refuses a colour defined in one theme only` asserts that `readTokens` given a dark theme without `link` throws an Error whose message contains `link` and `dark`.
- `refuses a colour that is not #rrggbb` asserts that `readTokens` given `ink: "black"` throws, naming `ink`.
- `generates camelCase keys in the TypeScript module` asserts that `generateTs` contains `inkMuted` and `statusWaiting`, contains no `ink-muted` key, and ends `as const`.

- [x] `feat(brand): generate css and typescript from one token source`

### Task 3: Contrast

Files: created `packages/brand/src/contrast.ts`; tested by `src/contrast.test.ts`
Produces: `contrastRatio`, `TextPair`, `TEXT_PAIRS`
Consumes: `readTokens` and `tokens.json` from Task 2

Tests:
- `matches the ratios brand.md records` asserts each of these to two decimals: Clay Text on Soft Sand 4.78, Midnight on Clay Coral 6.64, Soft Sand on Midnight 14.81, and Slate Focus on Soft Sand 3.62.
- `is the same either way round` asserts that `contrastRatio(a, b) === contrastRatio(b, a)` for Midnight and Soft Sand.
- `holds every pair to its minimum in the light theme` asserts that, for each `TEXT_PAIRS` entry, the ratio of its two light values is at least its minimum. On failure the message names the pair, the theme, and the ratio.
- `holds every pair to its minimum in the dark theme`: the same for dark.
- `lists every role colour against role-ink` asserts that for every `role-*` key in `tokens.json` other than `role-ink`, `TEXT_PAIRS` holds `role-ink` on it, so a role added to the tokens without a pair fails here.

- [x] `test(brand): hold every text and control colour pair to wcag aa`

### Task 4: Fonts, logo, icons, avatars

Files: created `packages/brand/src/fonts.css`, `src/icons.ts`, `src/assets.ts`, `src/index.ts`, `assets/logo-mark-1254.png`, `assets/wordmark-1024.png`, `assets/icons/icon-*.png`; moved `docs/brand/assets/avatars/*.png` to `packages/brand/assets/avatars/*-256.png`; modified `docs/brand/brand.md` (where the avatars and icons live); tested by `src/assets.test.ts`
Produces: `AVATAR_KEYS`, `ICON_SIZES`, the package's `exports`, and the `icons` script
Consumes: Tasks 2 and 3

Tests:
- `has every icon size at its size` asserts that for each `ICON_SIZES` entry, `assets/icons/icon-<n>.png` exists and its PNG header (IHDR, bytes 16 to 23) says `n` × `n`.
- `has a 256 px avatar for every key` asserts that for each of the ten `AVATAR_KEYS`, `assets/avatars/<key>-256.png` exists at 256 × 256.
- `keeps the founder's masters unchanged` asserts that the SHA-256 of `assets/logo-mark-1254.png` and `assets/wordmark-1024.png` equals that of the files in `docs/brand/assets/`.
- `bundles every font family the type tokens name` asserts that `fonts.css` imports a `@fontsource` package for each family used in `tokens.type`.

- [x] `feat(brand): add the fonts, logo, icons and avatars`

### Task 5: The brand sheet

Files: created `packages/brand/sheet/index.html`, `sheet/sheet.ts`, `sheet/vite.config.ts`; tested by `sheet/sheet.test.ts` (jsdom)
Produces: `renderSheet`, and the `sheet` script
Consumes: Tasks 2 to 4

Tests:
- `shows a swatch for every colour in both themes` asserts that `renderSheet` renders exactly one `[data-swatch="<theme>:<name>"]` per colour token per theme, whose text contains `<name>` and the upper-case `<hex>`.
- `shows every checked pair with its ratio` asserts exactly one `[data-pair]` per `TEXT_PAIRS` entry per theme, whose text contains the ratio to two decimals (`contrastRatio(...).toFixed(2)`) and the minimum.
- `shows every icon and avatar` asserts one `img` per `ICON_SIZES` entry and per `AVATAR_KEYS` entry, each with non-empty `alt` text.
- `has no axe violations` asserts that `axe-core` finds no violation in the rendered sheet.

- [x] `feat(brand): add the brand sheet for sign-off`

## Verification

```
cargo xtask check
# expected: the Rust checks as before; then pnpm's Vitest summary for @farik/brand,
#   "Test Files  4 passed (4)" and "Tests  27 passed (27)" (generate 7, contrast 8, assets 5, sheet 7: the
#   plan's 19, the 6 the landing review added, and the 2 of the palette change; the 2 xtask tests are counted by cargo);
#   last line: xtask check: ok
pnpm --filter @farik/brand sheet
# expected: vite writes packages/brand/dist/sheet/index.html and its assets, exit 0
```

Then the founder's sign-off of the published sheet, recorded here as `Status: done (signed off by the founder, <date>)`.
