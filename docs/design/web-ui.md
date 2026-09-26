# Farik web UI design

Status: approved by the founder on 2026-09-26, who then asked for every page to be mocked up before any code. It is the design input to phase 6. Phase 6's step plans build from it, and step 02's component library takes its tokens from `@farik/brand` (phase 5 step 02).
Sources:
- the brand, `docs/brand/brand.md` and the founder's kit, `docs/brand/brand-kit.png`;
- the spec: section 2 (who it is for), 4 (journeys), 5.4 (Definition of Done), 10 (non-functional requirements) and 14 (brand);
- the project plan's phase 6 decisions.

It was made with the `frontend-design` skill the founder installed (2026-09-25), in that skill's two passes: first a plan (palette, type, layout, principles), then a review of that plan against the generic defaults it names, with each revision recorded below.

A mockup of five key screens in the brand is published for review, as a private design canvas the founder owns: https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf (Today with a dark-theme switch, Accept work, the board, the first-run team step, and Today on a phone). It uses art cropped from the kit, and the final art comes from the founder's separate files (`brand.md`, "Files still to come").

## Subject, audience, job

- **Subject:** a small team of AI agents building the user's product. It has a Product Manager, a Scrum Master, an Architect, a Developer and a Marketing Specialist. They work a sprint board, post in a team channel, and ask the user when a decision is the user's to make.
- **Audience:** first, a non-technical founder who can say what they want but cannot read code (spec 2). Second, a solo builder who can read a diff.
- **Primary job:** show the user, at a glance, what the team is doing and what is waiting on them, and let them decide it in plain language. Everything else (the board, the channel, costs, settings) supports that job.

The most characteristic thing in this subject's world is the team itself: five pixel people at a long table, each busy with something. So the app opens on the team, not on a chart.

## Pass 1: the plan

### Colour

The five brand colours, the three text shades, and the light and dark themes are those in `brand.md`. The UI assigns them these jobs:

| Token | Light | Dark | Job |
|---|---|---|---|
| `page` | Soft Sand `#F3E7D3` | Midnight `#161616` | The page background |
| `surface` | `#FBF6EC` | `#1F1F1F` | Raised areas: the request box, a dialog, a lane |
| `ink` | Midnight `#161616` | Soft Sand `#F3E7D3` | Text |
| `ink-muted` | `#5B5347` | `#B9AE9C` | Secondary text (checked for AA in step 02) |
| `rule` | `#D9CBB3` | `#333333` | Borders and dividers |
| `band` | Midnight `#161616` | `#0E0E0E` | The team band and the navigation rail, dark in both themes |
| `action` | Clay Coral `#D8896A`, with Midnight text | the same | The one primary action on a screen |
| `role-*` | Coral, Moss, Signal Blue, lavender, Midnight | the same | Role tags only |
| `status-*` | Moss Text (done), Signal Text (working), Coral Text (waiting on you) | the base colours | Status words, always with a word, never colour alone |

### Type

The families are those in `brand.md`, with the type scale from *The Elements of Typographic Style* (Bringhurst's classic scale):

| Step | Size / line height | Face | Use |
|---|---|---|---|
| display | 36 / 40 | Silkscreen | The first-run welcome only |
| title | 24 / 30 | Space Grotesk 600 | Screen titles |
| heading | 18 / 24 | Space Grotesk 600 | Section headings |
| body | 16 / 24 | Space Grotesk 400 | Everything a person reads |
| small | 14 / 20 | Space Grotesk 400 | Secondary lines |
| code | 14 / 20 | JetBrains Mono 400 | Task ids, costs, diffs, the request prompt |

Lines are kept under 70 characters. Headings are in sentence case.

### Layout

On desktop (1024 px and wider), a dark rail runs down the left. It holds the icon and six places:
- Today (the home);
- Board;
- Channel;
- Team;
- Costs;
- Settings.

The main area is a single left-aligned column, 720 px at most for reading screens, and full width for the board.

```
+------+-----------------------------------------------------------+
| [#]  |  THE TEAM BAND (midnight)                                 |
|      |  [PM]      [SM]      [ARCH]    [DEV]     [MKT]            |
|Today |  Mira      Sol       Ada       Dev       Kai              |
|Board |  writing   planning  resting   waiting   drafting         |
|Chan. |  a plan    sprint 2  until 3pm on you   the launch post   |
|Team  +-----------------------------------------------------------+
|Costs |  > Ask the team for something_                            |
|      |                                                           |
|Sett. |  Waiting on you (2)                                       |
|      |  [DEV] Accept the sign-in page      Review ->             |
|      |  [PM]  Approve the plan for "Invite a teammate"           |
|      |                                                           |
|      |  What moved since yesterday                               |
|      |  ...                                                      |
+------+-----------------------------------------------------------+
```

On a phone (360 px), the rail becomes a bottom bar of five places, with Settings under Team. The team band scrolls sideways, with the agents waiting on the user first, and the request box stays at the top of Today.

### Principles

1. **The team is the interface.** Every piece of work has a face on it: the avatar of the agent who did it or who is asking. The user deals with people, not with records.
2. **One boldness: the team band.** The dark band of pixel characters across the top of Today is the product's signature. It echoes the kit's hero and the office scene. Everything below it is quiet and flat.
3. **Plain words first; the machinery one click away.** Every human gate leads with the agent's plain-language summary and Farik's checks in plain words. "See the code changes" opens the diff. Nothing requires reading code (spec 5.4, the phase 6 decisions).
4. **The terminal is a voice, not a costume.** The `>` prompt and the block cursor appear in one place: the box where the user tells the team what they want. That is the one place the user "types to the machine", so the metaphor means something there.
5. **Pixel art is for people and the logo.** Characters, the icon, the wordmark and the welcome screen are pixel art. Controls, text, the board and the diff are clean.

## Pass 2: review against the defaults, and the revisions

The skill lists five looks that generated designs fall into. The founder's kit pins some axes, and the skill says the brief's own words win there. The table records each default, what the kit pins, and what this design chose on the axes left free.

| Default the skill names | What the kit pins | Decision |
|---|---|---|
| Warm cream background with a terracotta accent and a serif display | Soft Sand and Clay Coral are the founder's colours, and are kept. | No serif anywhere. Coral is used only for the single primary action and the PM tag, never as wash or decoration. Midnight bands carry most of the brand's weight, so the page does not read as "cream and clay". |
| Near-black with one acid accent | Midnight is in the palette. | The dark theme uses four role colours with meanings, not one accent. |
| Broadsheet: hairlines, zero radius, dense columns | Nothing. | Not used. |
| The SaaS card kit: identical rounded cards, one radius, grey shadows, gradients | The kit's panels are flat rounded cards. | Revised. The first plan put every section of Today in a card. Now there is one raised surface per screen (the request box, or the gate's decision bar). Lists are rows divided by rules, and nothing has a drop shadow. The radius follows the hierarchy: 0 for pixel-framed things (avatars, the icon), 4 px for controls, 8 px for the raised surface and dialogs. |
| Template chrome: tracked all-caps labels above every heading, middle-dot meta strings, monospace for small data labels, an arrow on every button | The kit's brand sheet uses tracked all-caps monospace labels. | Revised, and `brand.md` changed to match. Tracked capitals belong to brand surfaces (the brand sheet, the welcome screen, the icon lockups). The app uses sentence-case headings with no label above them. Monospace is kept for what really is machine text: task ids, costs, diffs, the request prompt. Meta goes in its own layout slots, not strings joined with dots. Buttons say what they do and carry no arrows (the "Review ->" in the wireframe is a placeholder for a plain "Review" button). |

One more revision came from the "remove one accessory" test. The first plan also blinked the cursor in the team band and gave every avatar an idle animation. Now the only motion that runs without the user doing anything is the cursor in the request box. Any other motion answers an event: when an agent's status changes, its avatar makes one two-frame hop. Both stop under `prefers-reduced-motion`.

## Screens

The screens map to phase 6's steps as follows:

| Screen | Step | What it shows |
|---|---|---|
| Welcome and first-run wizard | 04 | One question per screen, as a numbered sequence (it is one), with a safe default already chosen and one line on why Farik asks. Back and Continue. The welcome uses the office scene and the display face. The team builder shows the five characters with their suggested names. |
| Today | 03, 05 | The team band, the request box, "Waiting on you", and "What moved". |
| A human gate: approve a plan, or accept work | 05 | The agent's summary as a short letter signed by the agent's avatar. Then "What Farik checked": each exit criterion in plain words, marked passed or failed. Then "See the code changes" (collapsed). A decision bar with the primary action ("Accept the work" or "Approve the plan") and "Send back with a note". |
| A question from an agent | 05 | The question with its avatar, the choices the agent offered, and a free-text answer. |
| Help needed (an escalation) | 05 | What happened in plain words, what the agent tried, and the user's options. |
| Board | 06 | Lanes named for the kit's sprint board (To do, In progress, Review, Done) plus Planning before them and Stuck after In progress. Each task is a row showing its assignee's avatar, its title, and a status word. Filters are chips. Sprint controls sit at the top. |
| Task detail | 06 | Summary, checks, history, the diff, notes, and cost, in that order. |
| Channel | 07 | Chat with avatars. Ceremonies (standup, review, retro) are collapsible threads titled in plain words. |
| Team | 04 | Each agent's card-sized profile: avatar, name, role, persona, model, and pause or retire. Its advanced view holds permissions and rules. |
| Costs | 06 | Today's and this sprint's spend per agent, in dollars, with the harness metrics. |
| Settings | 03, 04 | Theme (light, dark, or match the system) and the Advanced switch. |

Lifecycle states map to plain words. Nothing shows the raw state names unless the user opens a task's advanced view:

| Plain word | States |
|---|---|
| Planning | `draft`, `refining` |
| To do | `ready`, `assigned` |
| In progress | `in_progress`, `rejected` (shown as "Being reworked") |
| Review | `verifying` |
| Done | `accepted` (`cancelled` is hidden behind a filter) |
| Stuck | `blocked` |

The mark "Waiting on you" cuts across the lanes. It sits on:
- an `escalated` task whose reason is `approval`, which is a plan to approve;
- a `verifying` task that needs the human's acceptance;
- an open question.

Every other `escalated` task is marked "Needs your help".

The spec's lifecycle (5.2) is the authority. Step 06's plan checks this mapping against it.

## Quality floor

These hold without being mentioned on any screen:
- responsive from 360 px, with no sideways page scroll;
- visible keyboard focus: a 2 px Signal Blue ring with a 2 px offset;
- `prefers-reduced-motion` respected;
- WCAG 2.2 AA for every text pair, checked by phase 5 step 02's contrast test;
- status never shown by colour alone;
- every string externalized (step 02).

## Every page, mocked up

On 2026-09-26 the founder asked for every page to be mocked up before any code. There are 33 screens. They are on the design canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf) in six pages, and their sources are in `docs/design/mockups/`. Each `.dc.html` file is one screen, and `canvas.json` is the layout. The images in them are the canvas's uploaded crops of the kit, referenced as `/_blob/` addresses, so they show only on the canvas. Phase 6 builds from these screens, and step 02's components are taken from them.

| Canvas page | Screens | Phase 6 step |
|---|---|---|
| Daily work | Today (`Main`, with a dark-theme switch), Accept work (`Gate`), `Board`, `TaskDetail`, `SprintStart`, `SprintView`, `Channel`, `OneOnOne`, `Costs` | 03, 05, 06, 07 (the one-on-one is phase 8 step 03) |
| Requests and approvals | `RequestFiled`, `Questions`, `ApprovePlan`, `PlanEditor`, `SendBack`, `HelpNeeded`, `AnswerQuestion` | 05 |
| First run | `SetupProject`, `SetupScan`, the team (`Welcome`), `SetupPermissions`, `SetupSpending`, `SetupFinish`, `SetupAdvanced`, `Connect` | 01 (the connect page), 04 |
| Team and settings | `Team`, `AgentEdit` (with the skills and connectors suggested for the role, spec 6.6), `ConnectorAdd` (adding GitHub to one agent: sign in, then review its tools), `Settings` | 03, 04; the skills and connectors are phase 8 steps 01 and 02 |
| Phone (360 to 390 px) | Today (`Phone`), `PhoneGate`, `PhoneBoard`, `PhoneChannel` | 03 to 07 |
| Website | `Site`, the public site on the domain (ADR 0017) | Phase 8 step 07 |

The mockups are not the spec. Where one disagrees with `docs/SPEC.md`, the spec wins, and the step plan that builds that screen records the difference. The `Connect` screen's two "Mockup: …" buttons only switch between its states, for review.
