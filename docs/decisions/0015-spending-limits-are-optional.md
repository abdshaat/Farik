# 0015. Spending limits are optional

Date: 2026-09-23
Status: accepted

## Context

Farik has shipped dollar limits since phase 1 (D3): a daily budget of 20 dollars that `team.schema.json` requires, a sprint budget of 15 dollars, and a team rule that caps a task's `max_cost_usd` at 5 dollars unless the human raises it. The team file's schema says there is no way to turn that cap off. Phase 3 then used the day's remainder as the stand-in for the sprint's in the Definition of Ready and the assignment gate, so a contract that was ready in the morning can fail readiness late in a UTC day.

On 2026-09-22, confirmed on 2026-09-23, the founder decided: "Do not cap spending initially. We will allow the user to set up their own limits, which is optional. The user will be able to use different models for agents."

Three options were on the table.

- Keep the shipped numbers as defaults and let a user raise them. That still caps spending, which is what the founder ruled out.
- Take the dollar limits out altogether. That removes a control some users want, and the founder asked for it to remain the user's choice.
- Make every dollar limit optional, with no default, and keep enforcing a limit the user sets exactly as today. This is the founder's wording.

The other limits in `docs/SPEC.md` 5.5 are session tokens, wall clock, tool calls, sessions per task, and rejected iterations. They bound a loop, not spending: they stop an agent that is going round in circles whatever the price of its model. They are not what the founder's decision is about.

The decision also changes what happens when a model has no price. SPEC 5.5 (added in 0.8) refuses a usage report for a model that no price table prices and records nothing, because a cost of zero would under-report spend. With optional limits and a model of the user's choice per agent, that refusal stops the work of any agent on a model the table has not caught up with. It does so to protect a limit that may not exist, and the usage it drops is exactly the record the user would want.

## Decision

Farik ships no dollar limit. The team's daily budget (`budgets.daily_usd`), a sprint's budget (phase 4), and the team rule `max_task_budget_usd` are each optional, have no default, and are left out of the team `farik init` writes. A limit that is set is enforced as it is today, and one that is left out is no limit of that kind. Zero is still refused: leaving the limit out is the way to have none.

The limits that bound a loop stay as they are, with their shipped defaults.

A contract's own `budget.max_cost_usd` stays required. The Product Manager writes it as the contract's estimate, the human approves it with the contract, and the Definition of Ready and an epic's breakdown are measured against it. It is not a limit Farik ships.

A usage report for a model that no price table prices is recorded with its tokens, a cost of zero, and `unpriced: true`. No dollar limit counts it. `farik doctor` and every start of `farik run` name each active agent's model that has no price, and say how to price it (`.farik/prices.json`).

## Consequences

A new project runs with no dollar ceiling, so a user who wants one has to set it. The setup screen (phase 5) offers the daily budget as an optional field rather than a required one. Section 10's first-day figure of twenty dollars becomes a property of the shipped models, not a cap.

Existing team files keep the numbers they already hold, and those numbers stay enforced. The Milestone 0 run's team sets its own.

Readiness and assignment stop failing late in the day for a team with no daily budget, because the stand-in for the sprint is then unbounded. A team that sets a daily budget keeps phase 3's behaviour until phase 4 step 02 replaces the stand-in with the sprint's own remainder.

The cost against this decision: an unpriced model's spend is invisible to every dollar limit and to the cost per accepted task (F17), which then reports less than was billed. The warnings are how the user finds this out, and adding the model's row to `.farik/prices.json` is the fix. A stricter rule was rejected: refusing to run while a limit is set and a model is unpriced. Every contract carries `max_cost_usd`, so that rule would have blocked task work on any unpriced model, which is what the founder's third sentence rules out.

Phase 4 step 02 inherits this decision. Its sprint `budget_usd` is optional with no default, and an unset one leaves `budget_state`'s sprint unbounded, as phase 3 already does.
