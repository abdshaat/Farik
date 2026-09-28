# 0020. A role-kits phase before the web launch

Date: 2026-09-28
Status: accepted

## Context

On 2026-09-27 the founder decided that every role must ship with a special set of skills and connectors that let it do its job well, the Marketing Specialist with marketing skills and creative connectors such as Higgsfield among them, and that this work is a phase of its own before the initial web launch.

Until now the plan gave each role a system prompt and one or two skills (phase 4 step 01), and phase 7 built the plumbing: MCP servers configured per agent with credentials in the OS keychain and tools tagged read-only or side-effecting (step 01), and skills loaded into sessions at three levels (step 03). Nothing in the plan filled that plumbing: which skills a Product Manager needs, which connectors a marketer needs, and how a non-technical user connects one without tagging a hundred tools by hand. The launch followed the plumbing directly.

Two facts constrained the design:
- A kit needs the plumbing first: a connector is an MCP server, its credential lives where step 01 keeps credentials, and a skill loads the way step 03 loads it. So the kits come after phase 7's infrastructure and before the launch.
- A connector that generates images or video spends the user's credits on another service. Spec 5.6 makes any call that changes state outside the sandbox an `external_effect`, approved by the human per call, which is right for publishing and wrong for a marketer making twenty images.

The options for placement were these:
- **Steps inside phase 7.** No renumbering, but the phase would grow to fourteen steps and one pull request, past what one review can hold.
- **A phase after the launch.** The founder ruled it out: the agents must be good at their jobs at launch.
- **A phase of its own between the ecosystem and the launch.** Phase 7 keeps the plumbing and loses the launch; the kits are phase 8; the website and the launch are phase 9. This is the chosen order.

The options for spending connectors were these:
- **Approval on every call.** Safe, and unusable for generation.
- **Tag generation as `network`.** Usable, and it hides spending from the human.
- **An allowance.** Generation stays `external_effect`, and the user sets, when connecting, how many calls per sprint are pre-approved; a call beyond the allowance asks. Publishing has no allowance and always asks. This is the chosen way.

## Decision

Insert a phase, Role kits, between the ecosystem and the web launch. The phases after phase 6 are: 7 Ecosystem; 8 Role kits; 9 Web launch; 10 Desktop; 11 Native mobile; 12 Premium. ADR 0018's order is amended accordingly.

Each role ships a kit, `roles/<role>/kit.yaml`, validated against `kit.schema.json`: the skills it carries, in the Agent Skills format the roles already use, and its connectors, each with its setup copy for the wizard, the credential it needs, every tool tagged `network`, `external_effect` or `denied`, and an allowance, per tool and per agent, where a tool spends the user's credits. The team builder shows each connector as an optional "Connect" step when the role is added. A connector's tool list is pinned in the kit, and a test fails when the service's list drifts from it.

The first cut of each kit is in `docs/design/role-kits.md`, and the founder amends it as the phase is planned.

## Consequences

Easier:
- At launch every role is equipped, and the non-technical user connects a service by signing in, never by tagging tools.
- Spending on a creative service is visible and bounded per sprint, in the same place as the model budgets.
- The pinned tool lists make a connector's drift a failing test rather than a surprise in a session.

Harder:
- One more phase and pull request before the launch; the launch moves from phase 7 to phase 9, and everything after it renumbers.
- Farik takes on a catalogue of third-party services it does not control. Each connector is a dependency on another company's API, terms and pricing, and the kit check in step 06 has to be repeated when a service changes.
- An allowance counts calls, not money, because Farik cannot price another service's credits. The user still reads the bill on that service. A batch tool, one call for many generations, gets no allowance and always asks.
- A tag is Farik's judgment about another company's tool. The drift test catches a changed list, not a same-name tool that gains a side effect, so every pin update re-reviews the tags by hand.
- Six steps and some twenty services before the launch is real scope; whether every connector ships before `v0.1.0` is open for the founder.
- Connectors pull in prompt-injection surface: everything a connector returns is untrusted content (spec 8.6), and a creative service's output is no exception.
- Some connectors will need the service's own developer registration; which ones, and whether Farik ships a shared client id or the user brings their own key, is decided per connector in the step plans.
