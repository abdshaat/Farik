# Farik brand

Status: approved by the founder on 2026-09-26, together with the web UI design (phase 5, step 01). The final art waits on the files listed at the end.
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

The palette has five brand colours. Each has a job:

| Name | Hex | Role |
|---|---|---|
| Midnight Terminal | `#161616` | Dark surfaces (the hero, terminal panels, the dark theme's background), and body text on light surfaces |
| Clay Coral | `#D8896A` | The primary accent: the wordmark, the logo frame, primary buttons, the `>` prompt, the Product Manager's tag |
| Soft Sand | `#F3E7D3` | The light theme's background, and text on dark surfaces |
| Moss Grid | `#6E8F76` | Secondary accent: success and "done", the cursor, the Scrum Master's tag |
| Signal Blue | `#5A8DFF` | Information, links on dark surfaces, "in progress", the Developer's tag |

**Text shades.** These are derived by Farik and approved by the founder on 2026-09-25. Each is the same hue made darker, and is used only for text and links on light surfaces, because the brand colours fall short of WCAG 2.2 AA there:

| Shade | Hex | On Soft Sand | Base colour on Soft Sand |
|---|---|---|---|
| Coral Text | `#A44D2B` | 4.68:1 | 2.23:1 |
| Moss Text | `#536C59` | 4.70:1 | 2.94:1 |
| Signal Text | `#0653FF` | 4.66:1 | 2.57:1 |
| Amber Text | `#856015` | 4.67:1 | 1.93:1 (Amber `#E3B04B`, the Architect's tag) |

These combinations were measured:
- On Midnight Terminal: Clay Coral 6.64, Soft Sand 14.81, Moss Grid 5.04 and Signal Blue 5.76. All brand colours may be text on dark surfaces.
- Text on a Clay Coral, Moss Grid or Signal Blue fill is Midnight Terminal (6.64, 5.04 and 5.76). Soft Sand on Clay Coral is 2.23 and is never used.

**The Marketing Specialist's tag** is lavender in the kit, which is not in the palette. It is recorded as a tag colour, `#A99BF0` (read from the kit; to be confirmed with the character files), and is used only as a character's tag fill with Midnight Terminal text.

**The Architect's tag is Amber `#E3B04B`.** On 2026-09-26 the founder decided the Architect gets its own colour, because the moss green on its card (`#7F9A7F`) could not be told apart from the Scrum Master's. Amber was chosen for three reasons:
- **It is distinct.** Its hue is one no other tag uses, and it is lighter than all of them (relative luminance 0.48, against 0.24 to 0.38), so it stays distinct for colour-blind readers.
- **It carries Midnight Terminal text** at 9.12:1.
- **It echoes the office scene's** warm lamps and sticky notes.

Its text shade for light surfaces is Amber Text `#856015`, at 4.67:1 on Soft Sand. Like the other tag colours, Amber is used only as a tag fill and for status marks tied to the role, never as a general accent.

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
| Product Manager | PM, Clay Coral | Defines vision and priorities. |
| Scrum Master | SM, Moss Grid | Keeps the team aligned and unblocked. |
| Developer | DEV, Signal Blue | Builds, tests and ships features. |
| Marketing Specialist | MKT, lavender | Creates content and drives growth. |
| Architect | ARCH, Amber `#E3B04B` | Designs systems and technical foundations. |

The founder's character files of 2026-09-26 replace the kit's drawings:
- the Product Manager, Scrum Master, Developer and Marketing Specialist are full-body figures on transparent backgrounds;
- the Architect is supplied as a finished card: a seated figure at a laptop beside a whiteboard, with its tag and description.

The characters are the agents' default avatars, which the user may change (spec F1). In the web UI they appear as square avatars cropped to the head and shoulders.

## Pixel art: where it appears

These use pixel art:
- the logo and the wordmark;
- the characters and avatars;
- large display headings;
- the office scene (phase 7);
- small decorative touches, such as the blinking block cursor and the frame corners.

The rest of the interface is clean and flat: forms, tables, the board, the channel, and dialogs. The kit shows this mix. Its panels are flat cards with monospace section labels, and its "brand core" icons are simple filled glyphs on dark tiles.

## Environment

The office scene is a warm pixel-art room: wooden desks, plants, hanging lamps, a window, and a kanban board on the wall with the columns To do, In progress, Review and Done. The team sits at one long table. It is the desktop app's scene (phase 7), and the web app uses a still crop of it on the first-run screen.

## Files

The founder supplied these on 2026-09-26. They are kept unchanged in `docs/brand/assets/`, and phase 5 step 02 builds `@farik/brand` from them:

| File | What it is |
|---|---|
| `logo-mark.png` | The mark, 1254 × 1254 px, on a transparent background. It is also the source of the app icons. |
| `wordmark.png` | "FARIK" in Soft Sand, 1024 × 290 px, on a transparent background, for dark surfaces. On a light surface it sits on a Midnight Terminal tile, as the mark does. |
| `characters/product-manager.png`, `scrum-master.png`, `developer.png`, `marketing-specialist.png` | Full-body figures, about 200 × 300 px, on transparent backgrounds |
| `characters/architect-card.png` | The Architect as a finished card, 1086 × 1448 px, on a background. A transparent figure like the others would let it be used the same way. |

The kit's rule that the wordmark is always Clay Coral gives way to the supplied file: the wordmark is Soft Sand on dark surfaces. The README's banner and team images, in `docs/brand/readme/`, are composed from these files.

Still wanted:
1. **All five characters redrawn seated at a laptop.** The founder is regenerating them (2026-09-26). Each should be a transparent PNG at its native pixel size, in one shared pose and scale so they line up. These replace the files above, and the README's team image and the web mockups are rebuilt from them.
2. **For phase 7 only:** the office scene as layered pieces, and the characters' walking frames.
