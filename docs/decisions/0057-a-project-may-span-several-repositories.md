# 0057. A project may span several repositories, its documents kept in a local project folder

Date: 2026-10-12
Status: accepted (the founder's decisions of 2026-10-12 in conversation, recorded in `docs/design/multi-repo-projects.md`).

Amends:
- ADR 0051: a multi-repo project's documents are kept in a local project folder, committed only to that folder's own git repository and never pushed, not in `docs/catervas/` of any repository. A one-repo project is unchanged.
- ADR 0053: one process still drives one project at a time, and that project may be a project folder with several member repositories.
- ADR 0048, ADR 0049 and ADR 0051, and every ADR that names a phase from 9 on by its number: each of those phases moves down by one (Catervas Cloud is phase 10).

Merged step plans and earlier design documents keep the numbers they were written with. In a document dated before 2026-10-12, read a phase number from 9 on as one higher.

## Context

A Catervas project is one repository: the team's state (`.catervas/`), its documents (`docs/catervas/`, ADR 0051), every task's branch and `allowed_paths`, and one integration branch. Many businesses keep their software in several repositories, such as a product and its infrastructure, or a set of microservices. The founder asked for one team to work across interconnected repositories, bundled as one project in Catervas while each stays its own repository on GitHub, with the Architect understanding how they connect and drawing it.

The options for where a multi-repo project's state and documents live were:
- **One primary repository** holds them, committed, as today. This is the smallest change. The founder rejected it: "In microservice architecture, there is really no primary repo."
- **A dedicated documentation repository** that Catervas creates. This separates things cleanly but adds a repository the owner has to own.
- **A local project folder.** Every member is equal and nothing is pushed. The founder chose it ("Keep the documentation local rather than in a repo"), and chose to make it a git repository ("Local project folder, git") so that phase 8's history, staleness and approvals, all read from git, keep working.

For tasks, a task that changes several repositories at once would need branches in each, integrated together and undone together. The founder chose "One task, one repo": a cross-repository change becomes an epic with a task per repository, ordered by dependencies.

## Decision

A project is either a one-repo project, as today, or a multi-repo project.

A multi-repo project is a local project folder (`~/Catervas/<slug>/` by default) with its own git repository and no remote. It holds `.catervas/` and `docs/catervas/`, and lists 1 to 16 member repositories by name, path and integration branch in `.catervas/repos.yaml`. Members are added from local folders. Catervas writes nothing into a member except a task's branch.

Tasks:
- A task contract gains `repo`. A builder's task names one member. A docs task names none and works in the project folder.
- A task may depend on a task in another repository.
- Agents read every member and write only their task's worktree.
- The team's one integration policy applies to each member's own integration branch. Folder changes in the project folder are merged locally at once.

The Architect writes `architecture/system.md` (the system map), `interfaces.md`, `deployment.md`, and plans whose tasks span repositories. Each member's own documents are in the project folder under `repos/<name>/`.

The Catervafication covers every member, and a member added later gets its own. A one-repo project that adds a repository becomes a multi-repo project: its `.catervas/` and `docs/catervas/` move to the new project folder and are removed from the repository in one commit.

This is phase 9, Multi-repo projects, right after Catervas folders. Catervas Cloud and every later phase move down by one (project plan revision 44).

## Consequences

- A team can work across a product and its infrastructure, or a set of services, with one board, one channel and one budget. The Architect's documents show how the repositories connect, and a feature crossing them is planned with its order.
- A multi-repo project's documents are not on GitHub. Nobody else on the repositories sees them, and they are lost with the computer unless the owner backs up the folder or adds a remote by hand, which Catervas neither does nor reads. The Premium cloud copy covers them later. A one-repo project's documents stay committed, so the two kinds differ in where their documents travel. The Files page and the agents behave the same in both.
- `repos.yaml` holds absolute paths, so a project folder cannot be moved to another computer as it is. A moved member is found again with "Locate…".
- Every rule that reads "the repository" (worktrees, branches, the task id counter, the Definition of Done's diff, integration, read paths, the sandbox's mounts) must name which repository. That is phase 9's work.
- Turning a one-repo project into a multi-repo one removes `.catervas/` and `docs/catervas/` from that repository in a commit. The dialog says so in words before the owner confirms.
- `docs/SPEC.md` (3, 4.1, 4.4, 4.5, 5.3, 5.4, 5.6, 5.14, 5.16, 5.17, 6.3, 8.3, 8.4, 8.5) changes with the steps that build each part.
