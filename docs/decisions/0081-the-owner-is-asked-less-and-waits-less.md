# 0081. The owner is asked less and waits less

Date: 2026-10-20
Status: accepted (the founder, 2026-10-20: "please gather all the issues encountered while using it from a UX perspective and solve them"; the mechanisms decided by the planner the same day)

Amends SPEC 5.5 (the session budget), 5.9 (the team's prompts), 5.16 (the re-plan of ADR 0078), 8.3 (the sandbox image), 8.6 (the browser's connection). Follows ADR 0079, whose live run on the founder's project (epic CTV-1, a video studio) finished ten of eleven tasks on 2026-10-20 and found what is below.

## Context

- Every page load subscribed from the first event: the server sent the whole log (2996 events) one frame at a time before answering any query, so a page showed "Opening Catervas…" or "Loading…" for 4 to 66 seconds.
- Tasks needed system tools the sandbox lacks: headless Chrome's libraries (CTV-14), then `ffprobe` (CTV-19, CTV-20). Agents cannot install system packages and the owner had no setting for them. It cost four owner round trips and three budget escalations; a container hand-patched by the owner was replaced at the next session.
- A Designer task was refused for `preview_not_set`; the Product Manager asked the owner a free-text question, and the owner answered it ("I've set it…") without anything being set. The refusal told the owner to look under "How to open your app"; the Settings section is called "Your app".
- Ivo replaced Iris as the Designer. The Architect's review later said "Iris's work passed"; the Product Manager wrote "Iris can't start CTV-14". No prompt names the current team, and a retirement tells nobody.
- The re-plan of ADR 0078 ran six times in a row on the same board, each ending "I changed nothing", and asked the owner the same question twice. Its gate is in memory, keyed on the idle builders, which change whenever a session starts or ends.
- When the owner sent a task back to its plan (ADR 0079 decision 3), the refine session that followed counted against the task, and twice the task was escalated "spent 5 of 5 sessions" right after.
- A `sessions` escalation's help page showed tries and dollars, not the sessions it was about.
- A preview failure with no error line showed npm's update notices.

## Decision

1. **A page subscribes from now.** `subscribe` without `from_seq` starts after the newest event and answers `{ "head": <seq> }`; with `from_seq` (a reconnect) it streams from there as now. The server sends at most 200 events per round of its loop, so queries are answered between rounds. The events page for testing reads its rows with `events.since` from `head - 100`.
2. **The owner gives the sandbox system packages.** `team.yaml` gains `sandbox: { packages: [<Debian package>] }` (each matching `^[a-z0-9][a-z0-9+.-]*$`, at most 40). When the list is not empty, task and preview containers run `catervas/sandbox:<version>-<digest>-p<packages digest>`, an image built from the shipped one with the packages installed (`apt-get install -y --no-install-recommends`), built once when missing, before the container starts. A held sandbox whose list changed is recreated. Settings, "How work runs", gains "System tools in the sandbox": one package name per line.
3. **A Designer task refused for want of a preview waits on the owner as a row, not a question.** While the team has a Designer and no preview, and a `ready` task of the Designer's role exists, Today lists "Tell Catervas how to open your app" linking to Settings, Your app (the row the design review already uses). The refusal names "Settings, Your app".
4. **Every session knows the current team.** The prompt gains "Your team": each active agent's name and role, and "Anyone else named in your notes, memory or the channel has left the team." Retiring or replacing an agent posts a system line in the channel ("Iris has left the team." / "Ivo replaces Iris as UI/UX Designer.").
5. **A re-plan runs once per change.** The re-plan of an epic for a role runs only when no `plan` session of that epic started since the epic's children last changed (a `contract.written` or `task.transitioned` of a child) or the team last changed (`agent.updated`, `team.updated`). The in-memory key is gone.
6. **An owner's send-back does not cost the task a session.** The first `refine` session after each move into refining by the human is not counted, and the budget rule leaves a refining task alone while that session has not run.
7. **The help page of a sessions escalation shows sessions.** `task.tries` gains `sessions` and `max_sessions` (from the budget state); "About this task" shows "Sessions 6 of 6".
8. **A preview error without an error line shows its last lines without npm's `notice` and `warn` lines.**

## Alternatives rejected

- **Install packages in a running container with `docker exec -u 0`.** A task container without the network tier has no network, the preview runs in other containers, and a recreated container loses them.
- **Ship more packages in the image.** Every project would carry them; ADR's sibling PR #85 adds only headless-browser libraries, which web projects need commonly.
- **Let agents install system packages.** It gives every agent root in its sandbox.
- **Keep replaying history to the page.** No page uses it but the events page for testing.

## Consequences

- The first task after a package list is set waits for the image to build (about a minute).
- The re-plan may wait for a real change after a restart, instead of running at once.
