# 0012. The integration policy, and Farik's own pushes

Date: 2026-09-22
Status: accepted (2026-09-22, the founder)

Amends ADR 0004. Reverses D19 (`docs/plans/project-plan.md`, closed decisions).

## Context

D19 (2026-09-15) made `manual` the default integration policy: the human merges each accepted task, and the board shows it as awaiting integration until they do. `local_merge` had Farik merge locally "under `git_local`", and `pull_request` pushed the branch and opened a pull request, "which needs `git_remote`" (`docs/SPEC.md` 5.14). When phase 3 step 13 was planned, the founder weighed that default again. Three options were realistic.

Keep `manual` as the default. Farik never touches the integration branch, which is the safest choice. But every accepted task then waits for a git command from the human, which is the kind of work Farik exists to take off them. Every dependent task waits too, because a dependency has to be integrated before its dependent is assigned (D20).

Make a local merge the default, with no push. Dependents are unblocked as soon as the merge lands. But the remote falls behind until the human pushes, so anyone looking at it, including the same person on another machine, sees none of the team's work.

Merge and push by default, with pull requests as an opt-in for teams that review on the forge. This raises a question ADR 0004 left open: who is pushing? ADR 0004 and 5.6 tie pushing to `git_remote`, a tier held by an agent and granted to nobody by default. This push is not an agent's. No session asks for it. It runs after acceptance, from the orchestrator, under a policy the user chose in their team file. Requiring an agent's tier for it would mean either granting `git_remote` to an agent that never pushes, or inventing an actor whose only job is to hold the tier.

## Decision

The founder decided on 2026-09-22 that `auto_merge` is the policy a new project gets. It is the wire value `local_merge` renamed, with no alias. After acceptance Farik merges `farik/<id>` into the integration branch and, when the repository has a remote named `origin`, pushes the integration branch there. `pull_request` is opt-in: Farik pushes `farik/<id>` and opens a pull request with `gh`, every such pull request needs the human's approval and merge on the forge, and the task counts as integrated once it is merged. `manual` is kept: Farik does nothing until the human runs `farik integrate`. These pushes and pull requests are Farik's own actions on the host, taken with the user's own git and `gh` credentials under the policy the user chose. They are not an agent using `git_remote`; that tier still governs what an agent's session may do.

## Consequences

Accepted work reaches the integration branch without a git command from anyone, and dependents are assigned as soon as it does. A failed merge, push, or pull request becomes an escalation with reason `integration` on a task that stays `accepted`, because nothing leaves `accepted` (5.2). That makes `EscalationReason::Integration` reachable, which closes the open item phase 1 recorded. A failed push leaves the merge in place, because dependents start from the local integration branch.

Farik now pushes on the user's behalf by default, with the user's credentials, even when no agent holds `git_remote`. That is the kind of effect outside the machine that 5.6 treats as able to hurt, and it is why D19 chose `manual`. The founder weighed the one-line way out (`integration: manual` in `.farik/team.yaml`) against a team that waits on its human for every task. Because the choice is made by default, the user has to be told: the setup screen and `farik init` say that `auto_merge` pushes to `origin` (5.6).

What gets pushed is the integration branch, not only the merge commit. Commits the user made on their local integration branch and had not pushed yet go out with it. A user who keeps unpushed work on that branch should choose `manual` or `pull_request`, or keep that work on another branch.

Teams that choose `pull_request` take on `gh` as a dependency and are limited to the forges `gh` drives. `gh` picks the base repository itself, which in a fork can be `upstream` rather than `origin`.

ADR 0004 is amended to match. `git_remote` governs agents' sessions, and Farik's integration pushes are outside it. The credentials that let Farik push stay on the host, just as the credentials for the model API do.
