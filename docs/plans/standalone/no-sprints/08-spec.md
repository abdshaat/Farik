# Standalone CTV-9, plan 8: the spec records it (revision 0.116)

Status: draft
Branch: `feat/no-sprints` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md`, every section below; hard rule 8
Depends on: plans 1 to 7 of this folder (committed on this branch); `fix/plans-need-no-review`'s revision 0.115 on the branch after the rebase
Readiness confirmed by: <name>, <date> (one round, against `docs/standards/workflow.md` stage 2)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

`docs/SPEC.md` describes the product this branch built: no sprints, deliveries and their
ceremonies, escalation choices that say what they do, WIP that a stuck task does not fill, the
hand-over of a cleared task, and how a project made before keeps working. The revision note says
so. The past revision notes stay as they are (history). Words only, no code, no test.

## Decisions

- One revision, 0.116 (0.115 is ADR 0061's), dated the day the task runs, appended after the last
  revision line (line 19 on `main`, after 0.115's once rebased), in the file's format: "Revision 0.116
  (<date>) records that deliveries replace sprints (standalone plans `no-sprints`, ADR 0062, epic
  CTV-9): sprints, sprint planning, the standup, sprint budgets, the Backlog hold and
  `plan_in_sprints` leave the product; a delivery closes when an epic is accepted, five standalone
  tasks are accepted, or the team has no work left, recorded as `delivery.closed`, and gets one
  review, the Product Manager's and the Architect's refreshes, a changelog task Catervas files, and
  one retro; every escalation names its choices, says what each does next and suggests one, and is
  answered with one press or one printed command; a blocked, escalated or waiting task leaves its
  agent free, and a cleared one resumes with its agent or is handed over (`task.reassigned`);
  allowances and unprompted messages count per UTC day; old team files, templates and logs keep
  loading; the commands `sprint_start`, `sprint_end`, the queries `sprint.current`, `sprints.list`,
  `sprint.get`, `backlog.summary`, and `catervas sprint` are gone; two event kinds (3, 4.1, 4.2,
  4.4, 5.2, 5.3, 5.5, 5.7, 5.8, 5.9, 5.11, 5.12, 5.14, 5.16, 5.17, 6.1, 6.2, 6.3, 6.5, 6.6, 6.7,
  6.9, 7, 8.2, 8.4, 8.5, 8.6, 10, 11, 12, 13; F3, F7, F11, F12, F17)." The event-kind total in
  8.5 goes up by two.
- What stays about sprints in the spec, each said once, in 8.4 and 8.5: logs made before ADR 0062
  may hold `sprint.started`, `sprint.planned`, `sprint.ended` and sessions in the `planning` and
  `standup` threads, which Catervas reads and never writes; `team.yaml`'s `plan_in_sprints` and
  `ambient_messages_per_sprint` are read and not written; a contract's `sprint` field is read and
  ignored; `.catervas/sprints/` is left as it is. Nothing else in the spec says sprint, standup or
  Backlog (as a hold).
- 6.9 (the DevOps Engineer, not built): "a sprint's start approves its deploys" and the incident
  fix's exception are replaced by one sentence: "How a deploy is approved without sprints is decided
  when phase 13 is planned (ADR 0062)." Chose this over inventing an approval here: the DevOps
  Engineer is planned, not built, and its plans are re-planned in phase 13.
- 11 (milestones): Milestone 1's "team sprint" becomes "the team's work"; that the milestone run
  shows a delivery's review and retro.

## File map

```
docs/SPEC.md        modifies: the sections of Tasks 1 to 3, and the revision note (Task 3)
```

## Tasks

Each task reads its sections through, replaces every sprint, standup, sprint budget, sprint
planning and Backlog-as-a-hold sentence with what the code now does (plans 1 to 7 are the source;
ADR 0062 the reason), and keeps every other sentence. Where a sentence was "(added in 0.NN)",
the replacement says "(changed in 0.116)".

### Task 1: concepts, journeys and the glossary (sections 1 to 4, 13)

- 2: "takes the backlog grooming" stays (the Product Manager's list).
- 3: the **Sprint.** paragraph and **Planning work in sprints** with its bullets become one
  **Delivery (changed in 0.116; ADR 0062).** paragraph: what closes one, `TASKS_PER_DELIVERY`,
  that it is not a setting, the once-per-delivery review, refreshes, changelog task and retro, the
  digest of open escalations in the review, that a new delivery waits for the previous retro, that
  a changelog task never counts. The raise's and the product plan's "skip the sprint queue"
  exceptions go (nothing holds work). Catervafication: "the first work of a project" and "the
  cadence at the retro of the delivery that documented it". Templates: no `plan_in_sprints`.
- 4.1: no "Plan work in sprints" switch, no "Your team works in sprints" box; "Start with a
  Catervafication".
- 4.2: the channel shows each delivery's review and retro; proposals "after the team's review";
  Today's "Waiting on you" row of an escalation opens its page, which names each choice, what it
  does and the suggested one, answered with one press.
- 4.4: Settings has no "Planning work" part; its effect lines go; a template takes no sprint answer.
- 13: "Ceremony: a structured team conversation, a delivery's review or retro." and a new entry
  "Delivery: the work accepted since the last one, closed by an accepted epic, five accepted
  standalone tasks or a team with no work left, and reviewed once (3)."

- [ ] `docs(spec): concepts and journeys without sprints, revision 0.116`

### Task 2: the harness (section 5)

- 5.2: the WIP rule (counted statuses, waiting rows not counted, `within_limit`'s order and
  `max(limit, 1)`), the resume and hand-over rule with `check_reassignment`'s refusals, "a task has
  one holder", and the rules' order (no "sprint rules"; the delivery rules first).
- 5.3: the Definition of Ready loses the remaining sprint budget.
- 5.4: "the sprint waits" goes.
- 5.5: the per-sprint row of the budgets table goes; allowances count per UTC day; sprint budget
  enforcement and planning retries go.
- 5.7: the choices table of plan 1 in words (reasons, choices, next words, the suggested one, open
  choices, the integration choice, the end-of-run command lines and `--extra-tries`); "start a
  sprint: ..." goes.
- 5.8: `team/retro.md` sections are headed `## D<n> (<date>)`; written at each delivery's retro.
- 5.9: the ceremonies are the review and the retro of each delivery; the review's facts (its
  tasks, cost, completion notes, what it spent, the digest); the refreshes; the retro; the
  `catervas_append_retro` refusal's words; `ambient_messages_per_day`; the order "review, retro";
  the cadence at the retro.
- 5.11: the `sprint` field is read and ignored.
- 5.12: `ambient_messages_per_day` replaces `ambient_messages_per_sprint`; no `plan_in_sprints`;
  `max_task_budget_usd` loses "bounded by the sprint budget".
- 5.14, 5.16, 5.17: no sprint queue; `folder_doc.proposed`'s `delivery_id`; the card's `review_id`;
  the delivery report at `docs/catervas/product/reports/D<n>`; the Catervafication opens no sprint;
  the changelog task Catervas files.

- [ ] `docs(spec): the harness without sprints`

### Task 3: roles, requirements, architecture, the rest, and the revision note

- 6.1: the Product Manager's changelog task is filed by Catervas at each delivery; delivery
  reports. 6.2: the Scrum Master runs each delivery's review and retro; its kit is
  `running-ceremonies`, `writing-escalation-digests`, `catervafying-the-repository`; no planning
  tool. 6.3, 6.5, 6.6: refreshes, delivery reports, costs by task, agent, day or purpose. 6.7:
  allowances per day. 6.9: the sentence of the Decisions.
- 7: F3 loses the sprint filter, sprint line, Start/End, the sprint page, the Backlog lane and its
  words, `backlog.summary` and `tasks.get`'s `backlog`; F7 the "Meetings in this sprint" panel; F11
  costs per task, agent and day; F12 "escalations and deliveries"; F17 per project, no
  `--sprint`, no sprint window.
- 8.2: "the next UTC day's standup" goes. 8.4 and 8.5: the Decisions' "what stays", the two new
  event kinds `delivery.closed` and `task.reassigned` with their bodies, `filed_as: changelog`,
  `retro.appended`'s and `folder_doc.proposed`'s `delivery_id`, migration `0016_counted_seq.sql`,
  the kind total. 8.6: "How work runs" has no "planning in sprints".
- 10: "running a full sprint costs" becomes "a delivery's work and its two ceremonies cost". 11: as
  the Decisions say. 12: the resolved question about a sprint's length reads "Resolved 2026-09-15,
  superseded 2026-10-16 by ADR 0062: there are no sprints."
- The revision note of the Decisions.

- [ ] `docs(spec): roles, architecture and requirements without sprints; revision 0.116`

## Verification

```
grep -n -i 'sprint\|standup' docs/SPEC.md | grep -v '^[0-9]*:Version 0\.2' | grep -v '^[0-9]*:Revision 0\.'
# expected: only lines of 5.12, 8.4 and 8.5 naming what an old log or team file may hold, and 12's resolved question
grep -c '^Revision 0.116 ' docs/SPEC.md
# expected: 1
cargo xtask check
# expected: xtask check: ok
```
