# 0018. Ship on the web first; desktop after the web release is tested

Date: 2026-09-27
Status: accepted; its phase numbers are shifted by 0020, which inserts a role-kits phase before the launch

## Context

ADR 0016 put the web UI before the desktop app, but it kept the launch after both. Phase 7 built the desktop app, and phase 8's `v0.1.0` release shipped the web and desktop apps together.

On 2026-09-27 the founder set a stricter order:
- Farik is first shipped on the web.
- After that it becomes a desktop app, and then native iOS and Android apps.
- The web product must be complete and tested before any desktop work starts.

The options were these:
- **Keep ADR 0016's order and launch both clients together.** This is one release. But desktop work would start before the web product had met real users, which is what the founder ruled out.
- **Launch on the web, then build the desktop app.** The ecosystem and launch phase moves ahead of the desktop phase. The desktop phase opens with a recorded check of the released web product with real users, and no desktop step starts until that check passes. This is the chosen order.

## Decision

The phases after phase 5 are, in order:
- 6, Web UI (unchanged).
- 7, Ecosystem and web launch: `v0.1.0` ships the `farik` binary with the web app embedded, and no desktop app.
- 8, Desktop. Its first step is the web release check: the whole `v0.1.0` feature set is run in the browser by the founder and at least three non-technical testers, and the founder signs it off. No other desktop step starts before that step passes. The desktop app then ships as a later release.
- 9, Native mobile, after the desktop app.
- 10, Premium.

## Consequences

Easier:
- Real users reach Farik one phase sooner, and what they find shapes the desktop app before it is built.
- The launch release shrinks: one binary per platform and no signed desktop bundles.
- The desktop phase reuses a web UI that users have already tested, so it stays a shell, the office scene, and packaging.

Harder:
- At launch, a user must start the local Farik before opening the browser. The desktop app was what removed that step, and it now comes later. So phase 7 must decide how a non-technical user installs and starts Farik without a terminal. This is open in the plan.
- The pixel office, one of the product's faces, is missing from the launch.
- Browser notifications are less reliable than native ones, and the launch has only those.
- Phase 9 moves one phase further away.
