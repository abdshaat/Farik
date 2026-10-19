# 0075. The team works towards the owner's goal

Date: 2026-10-19
Status: accepted (the founder, 2026-10-19, quoted in `docs/design/team-goals.md`: the request, "Measured, owner confirms" and "Daily budget only, Do not include any budgets other than daily ones"; the mechanism decided by the planner the same day)

Builds on:
- ADR 0062: deliveries, a quiet team.
- ADR 0063: given tasks.
- ADR 0064: parallel agents, the turn.
- ADR 0071: one agent alone.

Amends:
- **SPEC 3, 4.2, 5.5, 6.1, 8.2, 8.5 and F17**, and adds **5.18** and **F18** (revision 0.130).

## Context

The founder asked for a goal the agents "will keep working together until achieving it", for example "Reach 10 paying customers" or "Create a mobile application and reach 10k downloads".

Today the team works only on what is asked. It goes quiet once the board is empty, and nothing in Catervas holds an aim beyond one request. The founder settled two questions:
- Progress is "Measured, owner confirms": read from a connected service where one exists, else from the owner, and shown on Today. Only the owner marks a goal reached.
- The only limit is the daily budget: no goal budget and no deadline.

So the question is how to keep a team working towards an aim without a money limit of its own, and without a loop that spins.

## Decision

1. **A goal** is the owner's words, an optional target and unit, and, for one with a target, how it is counted:
   - a connector `server` an active agent has connected;
   - or the owner's own number.

   A goal with no number is reached by the owner's word alone. Its id is `goal-<n>`.
2. **One goal at a time.** `goal_set` is refused `goal_active` while one stands, active or paused. Rejected: several goals, which would split one board and one planner, crowd Today, and none of the founder's examples needs.
3. **Events only.** The goal lives in the log as six kinds:
   - `goal.set` (an edit records it again with the same id);
   - `goal.paused`, `goal.resumed`;
   - `goal.ended { outcome: reached | dropped }`;
   - `goal.measured { value, source: connector | owner }`;
   - `goal.planned { filed }`.

   `task.created` gains `goal`. Sessions gain the purpose `goal`. Rejected: a goal file under `.catervas/team/`, a second source of truth the log already holds.
4. **The owner's commands** are `goal_set`, `goal_edit`, `goal_pause`, `goal_resume` (also "Keep going"), `goal_end` (with `reached` or `dropped`) and `goal_report`, and the query is `goal.get`. Only `goal_end { outcome: reached }` marks a goal reached; no reading does. `goal_set` and `goal_edit` are refused `goal_needs_daily_budget` on a team with no daily budget. Since the daily budget is the only limit on spend, a goal never runs without one.
5. **The planning rule.** Last in a tick, before idle, the planner gets one `goal` session. The planner is the active Product Manager, or the one agent in solo. The rule needs all of these:
   - a goal that stands and is not paused;
   - a daily budget set and not spent;
   - the planner free and awake;
   - no product plan awaited;
   - **a quiet board**: no task but `accepted` or `cancelled`;
   - a goal not met (no reading at or above the target);
   - a goal not stalled.

   The session files the next one to five steps with `catervas_file_goal_work`, reads the goal's service when the planner holds it, and changes nothing in the project. When it ends, Catervas records `goal.planned` with the tasks the round filed; a session that ended `aborted` (a stop, or the day spent by another session, ADR 0064), `provider_limit` or `credential_refused` records none, so neither the day's limit nor the provider's refusal reads as a round that found nothing. Rejected:
   - planning on a timer, which spends on nothing;
   - planning beside open work, which piles work up.
6. **Filing.** In a team, a goal request is a draft that Catervas triages at once with the planner's size; the Product Manager refines it as any request, and a large one becomes an epic. In solo it is a small task given to the agent; a large size is refused. Rejected: `catervas_create_task`, which runs a triage session per request and files nothing a solo agent can move (ADR 0071: a draft that is not given waits in solo).
7. **Measuring.** Once per UTC day the reader gets one `goal` session offered `catervas_record_goal_progress` alone. The reader is the first active agent in team order with the goal's `server` connected. The owner's number is the command `goal_report`. Rejected:
   - Catervas calling a connector itself: Catervas never calls a service in no agent's session;
   - a task per reading, whose contract, worktree and criteria would wrap one tool call.
8. **Bounded without money.** A round waits for the round before it to be done (the quiet board). A goal is stalled when its last round filed nothing and nothing has come in since (a reading, an edit, Resume), or when five rounds (`GOAL_ROUNDS_BEFORE_CHECK_IN`) have filed work since the last progress. Progress is the owner's word or a reading above every earlier one. A stalled goal waits for the owner's Keep going, an edit or a reading. A session files at most five requests (`GOAL_WORK_PER_SESSION`). Both are constants that bound a loop, as a session's tool-call limit does; neither is a budget. Rejected:
   - a per-day cap on goal sessions;
   - a per-goal dollar budget (the founder: "Do not include any budgets other than daily ones");
   - asking the owner after every round.
9. **Outward acts are unchanged.** Posts outside the plan, paid calls and purchases still ask (ADR 0061, 0042). Catervas only opens pull requests, and the marketing budget's hard stop holds. This decision does not depend on the owner allowing merges, which is a separate feature.
10. **Today** carries one goal card after the request box, outside "Waiting on you". With no goal it is a quiet card with "Set a goal". With a goal it shows:
    - the words and the progress, with its source and age;
    - what the team is doing now;
    - Mark reached and Update the number;
    - Edit, Pause or Resume, and End under "More".

    Its details sit behind an ⓘ. Rejected: a Goals page, which is one more place for one item.

## Consequences

- Positive:
  - a team with a goal plans its own next work, round after round, day after day, and stops only for the owner, the day's budget or a stall;
  - the owner sees how far along the goal is on Today, counted from the service that knows.
- Negative:
  - each round costs a planning session, and a goal with a service costs one short measuring session a day;
  - a round's work usually ends in a delivery (the board goes quiet, ADR 0062), so its changelog, review, refreshes and retro run between rounds;
  - a goal request is triaged by its planner's size at filing, so the owner cannot overrule its size in the draft, as with a raise (5.16);
  - a goal whose work keeps failing waits five rounds before it asks the owner;
  - a goal that is never measured cannot show progress, and stalls for check-ins every five rounds.
- Neutral:
  - nothing changes for a team with no goal;
  - an older log reads as having none.
