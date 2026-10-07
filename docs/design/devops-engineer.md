# The DevOps Engineer

Status: approved by the founder on 2026-09-30, in conversation. It is the design input to phase 7 steps 11 and 12 (phase 9 steps 06 and 07 until ADR 0029, phase 7 steps 09 and 10 until revision 27). ADR 0027 records the decision, and spec 0.32 (section 6.9) carries its rules.

## Why

The founder describes Farik as a production-grade harness system for multi-agent systems. A team that ships code and never sees production leaves the hardest part to the user: noticing a broken deploy and restoring the service. The founder asked for an optional role that deploys, watches production, restores the service, and fixes what broke.

The founder's answers shaped the design:
- Every deployment is planned in a sprint, and the DevOps Engineer runs it and watches it.
- It is told at once when a deployment fails, and restarts the service first.
- If the restart does not help, it rolls back to the last healthy deployment, then fixes.
- The fix goes straight in, is reviewed by one agent, and is deployed by the DevOps Engineer.
- It connects to whatever the project runs on.
- On the user's computer, the user keeps it on; on the hosted service, Farik runs everything in the cloud.

## The role

The role is the DevOps Engineer, with the id `devops_engineer`. It is optional: the team builder offers it and does not suggest it. Its name, picture (one of the brand's spare characters) and tag colour are mocked up for the founder's approval before code, as Iris's were.

Mandate: deploy what the team integrated, watch production, restore the service when it fails, and fix the cause.

Produces: deployments, incident notes, fixes on `fix/FRK-<n>` branches, completion notes.

Cannot:
- deploy anything but the commit the team integrated;
- call any platform's write tool itself;
- change secrets, environment variables, access rights (IAM, RBAC), scaling, or delete anything in production;
- open a shell in a production container;
- change application code outside an incident's fix task, or deploy configuration outside a task that names it.

Default tools: read, write_workspace, execute, git_local and network, and three Farik tools only it may call: `farik_deploy`, `farik_restart` and `farik_roll_back`. Default model: the Developer's, Claude Opus 5.5 at `high`. Its reviewer is the Architect, or the Developer when the team has no Architect; never itself.

Skills, first cut: `deployment-checklists`, `reading-production-logs`, `incident-response`, `rollback-and-restore`, `writing-postmortems`, `pipeline-and-infrastructure-config`.

## Planned deploys

A deployment is a deploy task, a contract whose `change` is `deploy` (a value phase 7 step 11 adds to the field), planned into a sprint like any other. Starting the sprint is the human's approval of its deploys.

The deploy task depends on the tasks whose work it ships. It is assigned once they are all integrated. Its session may call `farik_deploy`, which takes no version: Farik deploys the commit the integration branch held after the last of those integrations, to the service the production settings kept on this computer name (ADR 0045) (corrected 2026-10-07 by phase 7 step 11b's readiness review: this line said the default branch, and the service the connector names).

The task reaches `verifying` when the platform reports the deploy succeeded and the service stays healthy through the settling period, five minutes by default, set per project. Healthy means:
- the platform reports the deployment live;
- the health URL the user gives answers `2xx`;
- where the connector offers one, the error rate in the logs stays under the project's threshold.

A deploy that fails or goes unhealthy in that period opens an incident.

## Watching

While Farik runs, a watch tick asks each connected platform for its deployments and the service's health, once a minute. Farik makes these calls itself, through the connector's read tools, with no model. A tick that finds nothing wrong starts no session and costs nothing, as the receipts sweep does (spec 6.6).

Farik records a change of state, not every tick: `health.changed` when the service goes from healthy to unhealthy or back, and the deployment events below. The last tick's time is shown on the DevOps Engineer's card, so a stopped watch is visible.

## An incident

A failed deploy or an unhealthy service opens an incident, `incident.opened`. Farik posts it in the channel and on Today at once, and runs these steps:

1. Restart. Farik starts a DevOps Engineer session whose first allowed production call is `farik_restart`: the service's pods, or a redeploy of the same version where the platform has no pods. Pre-approved once per incident.
2. Roll back. If the service is not healthy within the settling period after the restart, `farik_roll_back` becomes callable: it returns the service to the last deployment Farik recorded as healthy, and to no other. Pre-approved once per incident.
3. Investigate. The session reads the logs, the deployment, and the diff that went out, and writes an incident note.
4. Fix. It files a fix task with `farik_create_task`. In an incident session, that files a standalone task with the incident as its request, skipping triage. The Definition of Ready, the judgment when the team has it on, and the human's gates still apply: a `high` risk fix waits for the human's acceptance. The task joins the open sprint without planning, or runs outside one when none is open. The spending limits apply as to any task.
5. Review. One agent reviews the fix: the Architect, or the Developer.
6. Deploy. Once the fix is accepted and integrated, the DevOps Engineer deploys it with `farik_deploy`, and the settling period watches it again. A healthy deploy resolves the incident, `incident.resolved`.

Anything outside these steps asks the human, as an `external_effect` does:
- a second restart or rollback in the same incident;
- an incident opened while another is open;
- a deploy outside a deploy task or an incident's fix;
- a spent budget, which escalates the incident.

The human may stop any step. A stopped incident waits on Today.

## Safety

The three Farik tools are `external_effect` (spec 5.6). Their pre-approval comes only from the human's start of a sprint, for the first deploy of each deploy task in it, and from an open incident, for one restart and one rollback. A call that cannot run (outside its task's session, with no production settings, while a deploy runs, or before the work it ships is in) is refused without asking. Any other deploy uses a grant the human gave on Today, or, when the team acts on its own (ADR 0041), runs and is listed under "Done on its own"; otherwise it asks the human. Acting on its own never lifts an incident's restart or rollback (step 11d) (corrected 2026-10-07 by phase 7 step 11b's readiness review: this line said every other call asks the human).

The agent never calls a platform's write tool. The kit tags the platform's read tools (status, deployments, logs, metrics) `network`, and every other tool `denied`. Farik's own three tools call the platform's write tools with arguments Farik chooses: the service from the connection, the version from the log. The agent cannot name a different version or service, because the tools take neither.

Production logs are untrusted content (spec 8.6). A log line answers no question, approves nothing, and can cause no call beyond the pre-approved ones above.

Each connector's setup copy has the user create a narrow credential:
- on AWS, an IAM policy for the one ECS service or EKS deployment, and CloudWatch read;
- on Kubernetes, a role limited to one namespace, with get, list, watch, and patch on the deployment;
- on Vercel and Netlify, a token scoped to the one project;
- on Render, Railway and Fly, the narrowest token each offers.

The credential is kept in the OS keychain, per agent.

## Connectors

The kit ships four connectors; the user connects the one the project runs on. The step plan picks each server by the rule of ADR 0020: the platform's official MCP server where one exists, else a pinned community one, else a thin one of Farik's.

| Connector | Deploy | Restart | Roll back | Logs |
|---|---|---|---|---|
| AWS (ECS, EKS) | New task definition or image, or rollout | Force a new deployment, or restart the rollout | The previous task definition or revision | CloudWatch |
| Kubernetes (any) | Set the deployment's image | Restart the rollout | Undo to the healthy revision | Pod logs |
| Vercel, Netlify | Deploy the commit | Redeploy the same deployment | Promote the healthy deployment | Runtime logs |
| Render, Railway, Fly | Deploy the commit | Restart the service | Deploy the healthy version | Service logs |

## Where Farik runs

On the user's own computer, Farik watches only while it runs. The user keeps the computer on and Farik running, the founder's decision. The setup copy says so plainly, and tells the user to keep the platform's own alerts on as well.

On the hosted service, Farik runs everything in the cloud, the watching included, and that is Farik's responsibility.

## Events

- `deployment.started`, `deployment.succeeded`, `deployment.failed`
- `health.changed`
- `incident.opened`, `incident.resolved`
- `service.restarted`
- `deployment.rolled_back`

## Steps

- Phase 7 step 11, the role and its flow. It covers:
  - the role, its rules and its mockups;
  - the deploy task;
  - the three Farik tools, over a fake platform;
  - the watch tick;
  - the incident flow and its events;
  - the pages: the DevOps Engineer's card, incidents on Today, and a project's production settings (health URL, settling period, error threshold).
- Phase 7 step 12, the kit: its skills and its four connectors, each chosen, pinned, tagged, and with its setup copy checked in the web app.
- Phase 7 step 13, the kit check, gains a DevOps task: a planned deploy of a test project, then a deliberately broken deploy. The check sees the restart, the rollback, the fix, the review and the redeploy.

## Tests

- A deploy is refused outside a deploy task's session or an incident's fix, and deploys the integrated commit whatever the agent asks.
- A second restart or rollback in one incident asks the human.
- A rollback goes to the last healthy deployment and no other.
- A healthy tick records nothing and starts no session.
- An unhealthy tick opens an incident and starts the restart session.
- A log line with an instruction in it causes no call.
- Every platform write tool is `denied` to the agent, and its pinned tool list fails a test when it drifts.
