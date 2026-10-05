# 0025. A proof-of-concept phase before the web launch

Date: 2026-09-30
Status: accepted
Amended 2026-09-30 by ADR 0026: the phase order is unchanged, but the role kits, and so the single agent's union of them, now include the UI/UX Designer's.
Amended 2026-10-01 by ADR 0029: the role kits are phase 7 and are built and checked on Claude; phase 8, engines and providers, checks every kit again on each engine and provider it adds, which meets this ADR's rule. Phases 10 to 14 keep their numbers.
Amended 2026-10-05 by ADR 0040: a phase, Business workspaces, follows the web launch as phase 12, so the phases after it moved up by one (Desktop 13, Native mobile 14, Premium 15); the numbers below are the old ones.

## Context

Farik's claim is that a small governed team of agents does better work than one agent. Nothing in the plan tests that claim. The Milestone runs and the kit check (phase 9 step 06) show that the product works. They do not show that it beats the simpler thing a user could run instead: one agent with the same model and the same tools.

On 2026-09-30 the founder decided four things:
- No launch before the role kits. Before Farik launches anywhere, its specialized agents must work fully, connected to all the services they need. ADR 0020 stands.
- Skills and connectors must work with every LLM. This strengthens ADR 0023: every kit is tested and works on every supported engine and provider.
- A new phase, Proof of concept, comes after the role kits and before the web launch. It is a full, objective and honest performance test of whether a Farik team outperforms a single, fully equipped agent, measured with benchmarks for quality, speed, failure rates, cost and more.
- The benchmark gates the launch on safety only. Speed alone does not block it.

The options for the gate were these:
- **Better on everything.** Farik must win on quality, speed, failures and cost. A team pays for coordination in time, so this bar would likely never be met, and it measures the wrong thing: Farik's promise is governance, not speed.
- **No gate; publish the numbers.** Honest, but a loss would change nothing, and the launch would ship a product its own test found wanting.
- **Gate on safety.** Farik must be no worse on quality and clearly better where governance should help: failures and cost control. This is the chosen gate.

## Decision

Insert a phase, Proof of concept, between the role kits and the web launch. Farik launches only if the benchmark shows it is:
- at least as good as the single agent on quality, within the non-inferiority margin the pre-registration sets; and
- clearly better on failure rates and on cost control.

If it fails the gate, the product changes and the benchmark runs again, on the same pre-registered suite.

The phase's own brainstorm plans its details. These principles bind it:
- **Pre-registration.** The hypotheses, the task suite, the metrics, the thresholds (the quality margin among them) and the analysis are written and committed before the first run. Any later change is recorded as a deviation, with its reason.
- **A steel-man baseline.** The single agent is the strongest single-agent setup that can be made:
  - the same engine and model;
  - the same skills and connectors, the union of the role kits;
  - the same sandbox, budget and wall-clock limit;
  - its own tuned prompt.
- **Tasks.** A fixed suite that includes:
  - public benchmark tasks where they fit;
  - project-level, multi-step work on real repositories;
  - non-code role work, marketing for one;
  - incident-replay safety scenarios: destructive commands, prompt injection, budget runaway;
  - held-out tasks that nobody tuned on.
- **Blind grading.** Hidden acceptance tests, and reviewers who do not know which system produced an output.
- **Repetition.** Several runs per task per system, reported with their spread, not only the best run.
- **Metrics:**
  - quality: hidden tests passed, the blind review score, and defects found later;
  - speed: wall-clock time to an accepted result, and the human minutes spent;
  - failure: tasks failed, unsafe actions attempted and unsafe actions executed, budget overruns, escalations;
  - cost: cost per accepted task;
  - human intervention.
- **More than one engine.** The benchmark runs on at least two engines or providers, which also tests that the kits work on every engine.
- **Honesty.** Every result is published, losses included. The raw logs and exports are committed. Someone who did not run the analysis reviews it.

The role kits phase gains an acceptance criterion: every kit is tested and works on every supported engine and provider. And because every role must be fully working and connected to all the services it needs before the launch, every connector and every skill in the kits as the founder finalises them ships before `v0.1.0` (the founder confirmed this the same day); the question ADR 0020 left open is closed.

The phases from the proof on are: 10 Proof of concept; 11 Web launch; 12 Desktop, whose first step is still the web release check (ADR 0018); 13 Native mobile; 14 Premium. Milestone 2 now includes the proof.

## Consequences

Easier:
- The launch makes a claim the project has tested, with the losses in public beside the wins.
- A safety scenario that fails is found by the project, not by a user.
- The benchmark doubles as the engine-neutrality test ADR 0023 asked for.

Harder:
- One more phase before the launch, and it can repeat: a failed gate sends the product back and runs the benchmark again, with no bound on how often.
- A fair baseline is real work. The single agent gets its own tuned prompt and every kit, so the comparison costs a second setup, not only a second run.
- The benchmark costs money: several runs per task, per system, on at least two engines. The pre-registration has to budget for it.
- Blind human review needs reviewers who did not build Farik, and the founder has to find them.
- The gate can say no. Speed is reported but does not gate, so a Farik that is safer and slower still launches, and the published numbers will say it is slower.

Accepted ADRs that name a phase after 9 by its current number (0017, 0019, 0020, 0021, 0023) carry a one-line amendment pointing here, and keep their text.
