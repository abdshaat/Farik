# 0016. Brand, then web, before desktop; native mobile later

Date: 2026-09-24
Status: proposed

## Context

Until now the plan built the desktop app first (phase 5: the Tauri shell, the board, the office scene), then the ecosystem and the launch (phase 6), and left a web client to the premium tier (phase 7). It also treated the look as provisional (D13): one design ADR in phase 5, refined by the founder just before launch.

On 2026-09-24 the founder set four things:
- Farik is for non-technical users, and setting up and configuring a team must be easy for them.
- It ships on the web and on the desktop, and later as phone apps on iOS and Android.
- The brand (logo, colours, slogans) is specified before implementation goes further.
- After that, a working web UI is wanted early, for easier testing.

The founder then answered four questions:
- The brand gets a phase of its own before the web UI.
- The first web UI covers the whole working loop, and the pixel office waits for the desktop.
- Every human gate shows both a plain-language summary and the code diff.
- The phone apps come after the product has shipped and been tested, and they are native to each OS, not a web page on the phone.

The options were these:
- **Keep desktop first and add a browser build of the same UI.** This keeps the order, but testing still waits for the Tauri shell. The look would stay provisional, when the founder wants to set it now.
- **Web alongside desktop in one phase.** The phase would be too large to review.
- **A brand phase, then a web phase, then desktop.** The desktop wraps the web UI the web phase already built. This is the chosen order.

## Decision

The phases after phase 4 are, in order:
- 5, Brand: the founder supplies it, and it is recorded as assets and design tokens.
- 6, Web UI: served by the local Farik, covering the whole working loop.
- 7, Desktop: the same UI in Tauri, plus the office scene.
- 8, Ecosystem and launch.
- 9, Native mobile: iOS and Android, native to each OS, after launch.
- 10, Premium.

D13's provisional design system and phase 6 step 07's "design refinement" are superseded by the brand phase.

## Consequences

Easier:
- A tester, the founder among them, can use Farik in a browser with no terminal two phases earlier than planned.
- The desktop phase shrinks to a shell, packaging, and the scene.
- One brand drives every client from the first screen.

Harder:
- Implementation after phase 4 waits on the founder's brand input. Phase 5 cannot start until the brand brief exists.
- The web UI is served by the user's own machine at first. So the browser talks to a daemon on `localhost`, and its token and origin checks must be right from the start rather than added with the hosted tier.
- A phone cannot run agents. The mobile phase must decide how a phone reaches a running Farik, the user's own desktop over a secure link or the hosted tier, and the second choice would make it depend on premium. The order of phases 9 and 10 is revisited when phase 9 is planned.
- Native apps on two platforms mean two more codebases, which is the cost the founder chose over a web wrapper.
