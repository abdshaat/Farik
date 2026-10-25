# 0013. Budgets are hard stops, with two walls

Date: 2026-10-24
Status: proposed

## Context

An agent that works constantly spends constantly. Catervas makes spending limits optional (Catervas ADR 0015) because its user watches it on their own computer; Praeco runs unwatched on a server, for a company, and the hosted service pays the model bill up front. Managed Agents offers a session budget, a hard cap the platform enforces before each model request.

## Decision

- Limits per cycle, per day and per month, in integer micro-dollars. Model spend is the session's cost as Managed Agents reports it (`usage.list_cost`, public list rates, in cents) on each `session.usage` event; Praeco keeps no price table of its own. A cycle's limit is a whole number of cents, as the session budget takes. The month's limit is required before the first cycle runs; the day's defaults to a thirtieth of the month's; the cycle's to $1.50 and never above the day's.
- **First wall:** Praeco's own count. A cycle starts only when the day and month have room for its whole limit; on each `session.usage` event Praeco records the session's cost so far and interrupts the cycle at its limit.
- **Second wall:** every session is created with a session budget equal to the cycle's limit, so the platform stops new model requests even if Praeco's process has died.
- Allowances for `paid` tools: calls per workspace per month (Catervas ADR 0037); a call past its allowance asks.
- Reaching a limit records `budget.reached`, stops new cycles until the period rolls over or an owner raises it, and is said in the digest. On the hosted service, a plan's included amount is the month's default limit (phase 10).

## Consequences

The worst case of a bad day is one cycle's limit past the day's, and an owner always knows why work stopped.

What cuts against it: a cycle can be cut off mid-thought at its limit, losing unrecorded work; the job's instructions tell the agent to record as it goes, not at the end.
