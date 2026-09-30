# Farik: instructions for Claude Code sessions

Farik is an operating system for small teams of AI agents with a governance harness at its core. Read `docs/SPEC.md` before touching anything; section 5 is the product.

## Standards (mandatory)

- Workflow: `docs/standards/workflow.md`. Brainstorm, plan, execute under TDD, verify, review, finish. In that order.
- Code: `docs/standards/code.md`. Naming (branches, commits, files, identifiers, wire formats, events), style, and the toolchain.
- Decisions: `docs/decisions/`. Read the existing ADRs before proposing a change that touches architecture, tooling, or process. Add one when you make such a change.

If the superpowers plugin is installed, its skills implement this workflow; use them, except `writing-plans`: write a step plan by copying `docs/plans/step-template.md` (ADR 0008, ADR 0010). If the plugin is not installed, follow the workflow document by hand. Either way the rules below hold.

## Hard rules

1. No production code before a failing test. Watch the test fail for the right reason. Code written before its test gets deleted, not adapted.
2. No completion claim without fresh evidence. Run the check, read the output, paste it. "Should work" is not a status.
3. Planning is two-level: `docs/plans/project-plan.md` holds phases and steps; each step has its own plan at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`, copied from `docs/plans/step-template.md` and passed by one readiness review (`workflow.md` stage 2) before execution. Tick its checkboxes in the same commits.
4. Commits follow Conventional Commits with a package scope. One task, one commit.
5. `crates/core` (`farik-core`) does no I/O. Ever. `cargo xtask core-io` checks it.
6. Wire and file formats use `snake_case`; Rust fields match them; TypeScript uses `camelCase`; one mapping layer per crate or package at the edge.
7. Event kinds are `<entity>.<past_tense_verb>`.
8. When behavior changes, `docs/SPEC.md` changes in the same pull request.
9. Never skip, disable, or quarantine a failing test to get green.
10. Do not accept your own work. A pull request is reviewed by someone, or by a fresh session, that did not write it. Every step gets a landing review of its running code as it lands, and its bar is mutation: re-introduce the bug each test claims to catch and confirm the suite notices.
11. One phase is one branch (`phase/<n>-<name>`) and one pull request to `main`. Open it as a draft when the phase's first step is pushed, without waiting to be asked; mark it ready when the last step's verification passes. Its description explains what changed and why it was necessary, and follows `.github/pull_request_template.md`. Never leave a pushed branch without a pull request. Work outside a phase (a standalone fix, a docs change) gets its own branch and pull request the same way.

## Commands

`cargo xtask check` is the full check (format, clippy, tests, bare-TODO check, the core no-I/O check; plus the front end's `pnpm check` once it exists) once the workspace is scaffolded. Until the scaffold exists there is no check command; say so in any verification section rather than implying one ran.

## Current state

Phases 0 to 3 are merged (pull requests #4, #5, #6, #11, with fixes in #12 and #13). Phase 4 (the team: five roles, sprints, budget consequences, the channel, ceremonies, memory) is merged in pull request #14. Phase 5 (the brand, and the web UI's design with every page mocked up) is merged in pull request #16, without its step 02 (the brand's assets and tokens), which became phase 6 step 01 (ADR 0021). The founder decided on 2026-09-24 (ADR 0016) that Farik is for non-technical users and that all live testing, the Milestone 0 and 1 runs among it, is done in the web UI once the product side is built. Phase 6, the web UI, is being built on `phase/6-web-ui`, in eleven steps planned in the project plan's revision 20: `farik serve` as the always-on driver, a browser session from a one-time code, and a first run that picks the folder, checks the computer, and keeps the AI account's credential in the OS keychain (ADR 0021). Phase 6 finishes on Claude alone. The order after it, since ADRs 0018, 0020, 0023 and 0025, is: 7 Engines and providers, any AI account (Claude, OpenAI, and more) and any agent engine (open-source ones such as Hermes and OpenClaw included) with the same tools, skills, connectors and governor enforcement, planned from its own brainstorm and ADR after phase 6 lands; 8 Ecosystem; 9 Role kits, every role's skills and connectors, each kit working on every supported engine and provider; 10 Proof of concept, a pre-registered, blind benchmark of a Farik team against one fully equipped agent, whose gate the launch must pass: at least as good on quality and clearly better on failure rates and cost control, speed alone not blocking; 11 Web launch, `v0.1.0` with the web app alone; 12 Desktop, which starts only after the released web product passes a recorded test with real users, and carries the Finance Specialist's receipts intake at its step 02; 13 Native mobile; 14 Premium. The sixth, optional role, the Finance Specialist (ADR 0019), keeps private spreadsheet books, records the team's AI spending and reads Stripe from phase 8 step 02.
