# 0027. An optional DevOps Engineer that deploys, watches and restores

Date: 2026-09-30
Status: accepted
Amended 2026-10-01 by ADR 0029: the role kits are phase 7, so the role and its flow are phase 7 step 09, its connectors step 10 and the kit check step 11; the step references below are updated. Renumbered 2026-10-01 by the project plan's revision 27, which split phase 7 step 01 and added a sign-in step: steps 11, 12 and 13.
Amended 2026-10-05 by the founder (answering the DevOps plans' O1 to O3): Farik's `farik_deploy`, `farik_restart` and `farik_roll_back` reach each platform by whichever official path works there, the platform's own server where it can deploy (Vercel, AWS, Kubernetes), the platform's own API with the same key (Render, Netlify), or Farik's own small server (Railway, Fly); a platform whose narrowest key reaches a whole account or team is accepted, with setup copy that says so; and an incident's first restart is made by Farik at once, with no model and no session, the DevOps Engineer's session investigating after (phase 7 steps 11d, 12, 12b and 12c).
Amended 2026-10-08 by ADR 0048 (the founder: "Lets set up the infra repository as well as the cloud hosting, landing page, etc. in phase 8"): a phase, Farik Cloud, follows the role kits as phase 8, so the phases after phase 7 moved up by one (Engines and providers 9, Ecosystem 10, Proof of concept 11, Web launch 12, Business workspaces 13, Desktop 14, Native mobile 15, Premium 16); the numbers below are the old ones.
Amended 2026-10-09 by ADR 0049 (the founder: "Lets stop this phase after completing step 10g. We have to start the next phase"; "DevOps later, rest after Cloud"): phase 7 ends at step 10g, and the DevOps Engineer, phase 7 steps 11 to 12e, moves to the Ecosystem phase, now phase 11, as its steps 05 to 06e, after that phase's own steps 01 to 04 (11 to 11f are 05 to 05f, 12 to 12e are 06 to 06e); ADRs 0045 and 0046 stay reserved for its plans. The kit check, now phase 9 step 02, gains no DevOps task: the Ecosystem phase checks the DevOps Engineer's kit on each engine and provider that Engines and providers, now phase 10, has added by then. A phase, Ask or auto and the milestones, follows Farik Cloud as phase 9, so the phases after phase 8 moved up by one (Engines and providers 10, Ecosystem 11, Proof of concept 12, Web launch 13, Business workspaces 14, Desktop 15, Native mobile 16, Premium 17); the numbers below are the old ones.

## Context

On 2026-09-30 the founder described Farik as a production-grade harness system for multi-agent systems, and asked that the team be connected to production: reading its logs, watching every new deployment, and restoring the service when a deployment fails. The founder asked for an optional role to do it, with a role kit of its own.

Until now no role touched production. The integration policy `auto_merge` pushes to `origin` (spec 5.14), and whatever deploys from there is the user's own pipeline, outside the team's sight. Nothing notices when a deploy the team caused breaks the service.

The founder decided the following in conversation:
- Every deployment to production is planned in a sprint. The DevOps Engineer deploys it, watches the build, and makes sure the service runs properly afterwards.
- It is told at once when a deployment fails. It restarts the service's pods, where the platform has them, to get it running again. Then it investigates, fixes the cause, and deploys the fix once one reviewer agent has reviewed it.
- If the restart does not bring the service back, it rolls back to the last healthy deployment first, then fixes.
- The incident's fix goes straight in: it joins the open sprint, or runs outside one, without waiting for planning. One agent reviews it and the DevOps Engineer deploys it. The human is told at each step and may stop it.
- The agent connects to whatever the project runs on. The kit covers AWS (ECS and EKS), any Kubernetes cluster, Vercel and Netlify, and Render, Railway and Fly.
- On the user's own computer, the user keeps it on and Farik running; Farik watches only while it runs. On the hosted service, Farik runs everything in the cloud, the watching included.

The design is `docs/design/devops-engineer.md`.

Three facts constrained the design:
- A production credential is the most dangerous thing a team can hold. A single wrong call can delete a service or leak a secret.
- Production logs are written by the service's users as much as by the service. An attacker can put text in a log, so the logs are untrusted content (spec 8.6), and no line in them may cause an action.
- The pre-approval spec 5.6 gives an `external_effect` is per call and by the human. The founder wants a restart and a rollback without waiting for anyone.

The options for how the agent reaches production were these:
- **The platform's MCP tools, tagged.** The kit would tag deploy, restart and rollback `external_effect` and pre-approve them in some contexts. But the agent would still choose the arguments: which version to deploy, which service to restart.
- **Farik's own three tools over the platform's.** `farik_deploy`, `farik_restart` and `farik_roll_back` take no version. Farik picks the integrated commit, the service and the last healthy version, and calls the platform's write tools itself. The agent is offered the platform's read tools only. This is the chosen way.

The options for when Farik watches were these:
- **A session that keeps watching.** Simple, but it spends the model's tokens every minute.
- **A watch tick with no model.** Farik asks each connected platform for its status and health once a minute, as the receipts sweep does for the Finance Specialist (spec 6.6), and starts a session only when something is wrong. This is the chosen way.

## Decision

Add an eighth role, the DevOps Engineer (`devops_engineer`). It is optional: the team builder offers it and does not suggest it. It has the Developer's tiers and `network`, the Architect is its reviewer (the Developer when the team has no Architect), and it changes code only on incident fixes and on the project's deploy configuration.

A deployment is a deploy task in a sprint; starting the sprint approves its deploys. Farik watches production with a watch tick that uses no model. A failed deploy or an unhealthy service opens an incident, which restarts the service once, rolls back to the last healthy deployment once if the restart does not restore it, and files a fix that skips planning, is reviewed by one agent, and is deployed by the DevOps Engineer. A second restart or rollback, a spent budget, or any production change outside these paths asks the human.

The three Farik tools are `external_effect`, approved by the human's start of a sprint for its deploy tasks, and pre-approved once each by an open incident for the restart and the rollback; this amends spec 5.6's tier table. The incident's fix skips triage and sprint planning, but the Definition of Ready and the human's gates of spec 5.4 still apply, so a `high` risk fix waits for the human.

The agent never calls a platform's write tools. Farik's `farik_deploy`, `farik_restart` and `farik_roll_back` do, with arguments Farik chooses; the platform's read tools are `network`, and everything else is `denied`.

The role and its incident flow are phase 7 step 11; its four platform connectors are phase 7 step 12; the kit check becomes step 13 and gains a DevOps task (phase 9 steps 06, 07 and 08 until ADR 0029, phase 7 steps 09, 10 and 11 until revision 27).

## Consequences

Easier:
- The team finishes what it ships. A deploy the team caused is watched, and a broken one is restored without the human.
- The riskiest credential Farik holds reaches production only through three tools whose arguments the agent cannot choose.

Harder:
- Farik becomes an always-on process in earnest. On the user's own computer, a sleeping laptop is an unwatched production; the setup copy says so, and tells the user to keep the platform's own alerts on.
- Four platform connectors, each with a narrow credential the user must create: an IAM policy for one service, a Kubernetes role for one namespace, a project-scoped token. The setup copy has to make that possible for a non-technical user.
- A team of the six suggested agents and the DevOps Engineer is seven, the cap; adding the Finance Specialist too means unticking one.
- The benchmark of phase 10 has to cover incidents, or the role goes untested there.
