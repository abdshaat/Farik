# Farik brand

Status: approved by the founder on 2026-09-26, together with the web UI design (phase 5, step 01); the palette changed on 2026-09-28 (see Colour). The final art waits on the files listed at the end.
Source: the founder's brand kit, `docs/brand/brand-kit.png` (2026-09-25), and the founder's answers of the same day.
Where this file and the kit disagree, the kit is the founder's intent. This file is what the apps build from.

## Name and descriptor

- **Name:** Farik. It is written in capitals in the wordmark ("FARIK") and as "Farik" in running text.
- **Descriptor:** AI Harness Engine. It is set in capitals beside or under the wordmark.

## Taglines and voice

| Use | Line |
|---|---|
| Primary tagline | Configure AI teams that build together. |
| The loop (a four-line list, with a `>` prompt before each line and a blinking cursor after the last) | Plan. / Build. / Iterate. / Ship together. |
| Positioning | Multi-agent teams for real products. |
| Header strap | AI teams / Agile products / A brighter tomorrow |

The three brand pillars each have a short description:
- **Multi-agent orchestration:** coordinate specialized AI agents to work as a unified team.
- **Configurable role hierarchy:** assemble and customize your ideal product team.
- **Agile product execution:** plan, iterate and ship faster, together.

Voice: short, plain, confident sentences, with a terminal's economy. It speaks to a non-technical person: it says what happens and what to do next, and it uses no jargon that the screen does not explain. The terminal touches (the `>` prompt, the blinking block cursor, monospace labels) are decoration. The words themselves stay plain English.

## Colour

Changed on 2026-09-28 by the founder: muted colours, one per job; Signal Blue, the lavender and Amber left the palette.

**One colour per job.** No two jobs share a colour. The main button, each of the five role tags, each of the three statuses, and the focus ring all have their own colour, in both themes, and a test in `packages/brand` fails if two of them match. Links are the one exception to "a job has a colour": they are underlined text in the ink colour, so the underline marks them, and the components draw it (phase 6 step 03).

The kept base colours:

| Name | Hex | Role |
|---|---|---|
| Midnight Terminal | `#161616` | Dark surfaces (the hero, terminal panels, the dark theme's background), body text and links on light surfaces, and text on every fill |
| Clay Coral | `#D8896A` | The main button and nothing else, plus the wordmark's frame, the logo frame and the `>` prompt |
| Soft Sand | `#F3E7D3` | The light theme's background, and text and links on dark surfaces |
| Moss Grid | `#6E8F76` | "Done" on dark surfaces, and the cursor |

The new colours, each with one job:

| Name | Hex | Job |
|---|---|---|
| Dusty Rose | `#C9A0A6` | The Product Manager's tag |
| Sage | `#A3B8A0` | The Scrum Master's tag |
| Wheat | `#D6B77A` | The Architect's tag |
| Sky | `#B7C7DA` | The Developer's tag |
| Heather | `#B3A9CF` | The Marketing Specialist's tag |
| Slate | `#8AA3C2` | "In progress" on dark surfaces |
| Peach | `#E0A68C` | "Waiting on you" on dark surfaces |
| Mist | `#A7BCD6` | The focus ring on dark surfaces |
| Added tint | `#E3EBDF` light, `#243029` dark | The background of an added line in the code-changes view, under ink (2026-09-28) |
| Removed tint | `#F2DDD3` light, `#3A2822` dark | The background of a removed line in the code-changes view, under ink (2026-09-28) |

The Added tint has a second job, fixed by the founder on 2026-09-30: the `ready` pill behind "Ready" on the computer check, with Moss Text (`ready-ink`) as the word. Both are the same in the light and dark themes, like the role tags, because Moss Grid on the dark tint measures 3.83:1 and fails AA; Moss Text on the Added tint is 4.71:1, and `packages/brand` tests that pair.

The tags carry Midnight Terminal text, measured at 7.81 (Dusty Rose), 8.55 (Sage), 9.40 (Wheat), 10.51 (Sky) and 8.18 (Heather).

**Text shades.** These are the same hues made darker, and are used only for text, status marks and the focus ring on light surfaces, because the lighter colours fall short of WCAG 2.2 AA there:

| Shade | Hex | On Soft Sand | Base colour on Soft Sand | Job |
|---|---|---|---|---|
| Moss Text | `#536C59` | 4.70:1 | 2.94:1 (Moss Grid) | "Done" |
| Slate Text | `#44607F` | 5.33:1 | 2.12:1 (Slate) | "In progress" |
| Clay Text | `#96533A` | 4.78:1 | 1.72:1 (Peach) | "Waiting on you" |
| Wheat Text | `#7A6232` | 4.75:1 | 1.57:1 (Wheat) | The Architect, where its name or mark must be text on a light surface |
| Slate Focus | `#5F7A9B` | 3.62:1 | | The focus ring on light surfaces (3:1 is the floor for a focus indicator; it is 4.11 on a card and 4.09 on the band) |

On dark surfaces (Midnight Terminal) the measured ratios are: done 5.04 (Moss Grid) and 4.59 on a card, in progress 6.98 and 6.36, waiting 8.62 and 7.85, focus 9.31 and 8.48, and focus on the band 9.93. On light surfaces, on the page and on a card: done 4.70 and 5.33, in progress 5.33 and 6.04, waiting 4.78 and 5.42.

Text on a Clay Coral fill is Midnight Terminal (6.64); Soft Sand on Clay Coral is 2.23 and is never used. Soft Sand on Midnight Terminal is 14.81.

**Themes.**
- **Light is the default:** Soft Sand pages; Midnight Terminal for the hero, the top bar, and terminal-style panels; white-on-sand cards (`#FBF6EC`, a lighter tint of Soft Sand).
- **Dark is an option in settings:** Midnight Terminal pages; `#1F1F1F` cards; Soft Sand text. Both themes are fully designed.

## Type

The founder chose a clean sans for the UI, with monospace accents (2026-09-25). There are three families, each with its own job:

| Family | Use | Choice |
|---|---|---|
| Pixel display | The wordmark, and the largest headings only | The wordmark is drawn, not typed: an SVG pixel grid, rebuilt from the kit unless the founder supplies it. Headings use Silkscreen (SIL Open Font License), the closest free pixel face. |
| Sans | All interface text: body, forms, buttons, tables | Space Grotesk (SIL Open Font License) |
| Monospace | Taglines, the terminal touches (the `>` prompt and the cursor), task ids, costs, code and diffs. Section labels in capitals with wide letter-spacing, as in the kit, appear only on brand surfaces (the brand sheet, the welcome screen, the logo lockups); inside the app, headings are in sentence case (`docs/design/web-ui.md`, pass 2). | JetBrains Mono (SIL Open Font License) |

The fonts are bundled with the app and never loaded from a font CDN, because the product works offline (spec 10).

## Logo

The mark is four role faces around a gear, in a Clay Coral pixel frame on Midnight Terminal. The faces are the Product Manager (brown hair), the Developer (blue cap), the Scrum Master (glasses), and the Marketing Specialist (green cap).

The kit shows three lockups:
1. **Primary (stacked):** the mark, with "FARIK" to its right and "AI HARNESS ENGINE" under the wordmark.
2. **Icon only:** the mark on its Midnight Terminal rounded square. This is the app icon, and it serves as the favicon at small sizes.
3. **Horizontal:** the mark, then "FARIK", then a thin rule, then "AI HARNESS ENGINE".

Rules:
- The wordmark is the supplied Soft Sand file on dark surfaces (see Files). The kit showed it in Clay Coral with a darker pixel shadow.
- On light backgrounds, the mark keeps its Midnight Terminal tile.
- Nothing else sits within clear space equal to one pixel-cell of the mark times four.

## Characters

The characters are pixel-art people, one per role, each with a coloured role tag and a one-line description:

| Role | Tag | Description |
|---|---|---|
| Product Manager | PM, Dusty Rose | Defines vision and priorities. |
| Scrum Master | SM, Sage | Keeps the team aligned and unblocked. |
| Developer | DEV, Sky | Builds, tests and ships features. |
| Marketing Specialist | MKT, Heather | Creates content and drives growth. |
| Architect | ARCH, Wheat | Designs systems and technical foundations. |

The founder's character files of 2026-09-26 replace the kit's drawings. There are ten pixel-art people, each seated cross-legged at the same laptop, drawn in one pose and one scale on transparent backgrounds:
- five are the roles' default characters: the Architect, Product Manager, Scrum Master, Developer and Marketing Specialist;
- five more are extra characters any agent may wear.

The characters are the agents' default avatars, which the user may change (spec F1). In the web UI they appear as square avatars cropped to the head and shoulders (`packages/brand/assets/avatars/`).

## Pixel art: where it appears

These use pixel art:
- the logo and the wordmark;
- the characters and avatars;
- large display headings;
- the office scene (phase 12);
- small decorative touches, such as the blinking block cursor and the frame corners.

The rest of the interface is clean and flat: forms, tables, the board, the channel, and dialogs. The kit shows this mix. Its panels are flat cards with monospace section labels, and its "brand core" icons are simple filled glyphs on dark tiles.

## Environment

The office scene is a warm pixel-art room: wooden desks, plants, hanging lamps, a window, and a kanban board on the wall with the columns To do, In progress, Review and Done. The team sits at one long table. It is the desktop app's scene (phase 12), and the web app uses a still crop of it on the first-run screen.

## Files

The founder supplied these on 2026-09-26. They are kept unchanged in `docs/brand/assets/`, and phase 6 step 01 builds `@farik/brand` from them:

| File | What it is |
|---|---|
| `logo-mark.png` | The mark, 1254 × 1254 px, on a transparent background. It is also the source of the app icons. |
| `wordmark.png` | "FARIK" in Soft Sand, 1024 × 290 px, on a transparent background, for dark surfaces. On a light surface it sits on a Midnight Terminal tile, as the mark does. |
| `characters/architect.png`, `product-manager.png`, `scrum-master.png`, `developer.png`, `marketing-specialist.png` | The roles' default characters, seated at a laptop, each 1254 × 1254 px on a transparent background |
| `characters/extra-1.png` to `extra-5.png` | Five more characters in the same pose and scale, for any agent |

The 256 px square avatars of head and shoulders (`<key>-256.png`, ten of them) were derived from the characters by Farik, not supplied. They live in `packages/brand/assets/avatars/`, moved there from `docs/brand/assets/avatars/` in phase 6 step 01. The app icons (`icon-<n>.png` for 16, 32, 48, 180, 192, 512 and 1024 px) are a Lanczos resize of `logo-mark.png`, made by `pnpm --filter @farik/brand icons` and committed in `packages/brand/assets/icons/`.

The kit's rule that the wordmark is always Clay Coral gives way to the supplied file: the wordmark is Soft Sand on dark surfaces. The README's images (`docs/brand/readme/`: the banner, the team, and the avatars) are composed from these files.

Still wanted:
1. **For phase 12 only:** the office scene as layered pieces, and the characters' walking frames.
