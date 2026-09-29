# 0024. The human may send work back and grant more tries

Date: 2026-09-29
Status: accepted

## Context

The founder approved two web UI mockups on 2026-09-26.
- **SendBack.** The human sends finished work back with a note. It says "This is try 1 of 3. After the third, Farik stops and asks you".
- **HelpNeeded.** When a task has used its tries, it offers "Give Theo 2 more tries".

The governor could do neither:
- The human's rows in the transition table (spec 5.2) are "any to escalated", "any to cancelled", and "escalated to any". A result waiting on the human's acceptance could not be rejected by the human.
- The try limit (`max_iterations`) was fixed per contract. A human who resolved an iterations escalation back to work gained one uncounted attempt, and nothing more.

## Decision

The founder's approval of these mockups is the decision. The mechanism is recorded here.
- **Sending work back.** The human may move a result `verifying → rejected` through a new gate, `HumanRejection`. The gate opens only while the result waits on the human's acceptance, and only after the reviewer's review has passed; for an epic it opens at any time. It needs the human's message; naming failed criteria is optional. The rejection counts as a try, like a reviewer's. The reviewer still reviews first, so the human does not stand in as the reviewer (spec 5.1).
- **More tries.** A human resolving an `iterations` escalation back to work may add `extra_tries` (1 to 5). The iteration limit becomes `max_iterations` plus the sum of the task's extra tries. The session allowance grows by 4 sessions per extra try, so a longer run is not stopped by the sessions limit first (spec 5.5).

## Consequences

- **Spec.** Spec 5.2 gains the row. Spec 5.4 says what the human's send-back is. Spec 5.5 and 5.7 give the extra-tries arithmetic.
- **Cost.** A task given more tries can cost more than its first budget suggested. Its dollar budget still holds (spec 5.5), so a spent budget escalates as before.
- **Log.** `escalation.resolved` carries `extra_tries`, so the history shows why a task ran past its limit.
