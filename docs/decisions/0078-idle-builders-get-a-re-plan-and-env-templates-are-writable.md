# 0078. Idle builders get a re-plan, and env templates are writable

Date: 2026-10-20
Status: accepted (the founder, 2026-10-20, quoted below; the mechanism decided by the controller the same day)

The founder: "This task keeps failing and theo keeps asking for my help. Please add more parallisim to the agent's work and make agents able to work in parallel on tasks."

Amends:
- **SPEC 5.6** (protected paths): the shipped `.env.*` no longer protects an env template.
- **SPEC 5.3** (Definition of Ready): a new rule, `protected_path_allowed`.
- **SPEC 5.2** (the transition table): a new row, `ready -> refining` by the Product Manager.
- **SPEC 5.16 item 3** and **ADR 0051** (lanes): an epic whose waiting tasks leave a builder idle is re-planned.

## Context

In the founder's project, CTV-11 asked for `.env.example`, the four keys the app needs with empty values. The shipped protected glob `.env.*` matched it, and the shipped globs cannot be taken out in the team file, so no tool could write it: the Developer built everything else in one session and then declared the task blocked eleven times in a row, each time asking the owner to write the file. Readiness had passed the contract, since nothing compared its allowed paths with the protected ones.

In the same project, an epic's eight Developer tasks were planned as one lane, each depending on the one before. A second Developer added later never had a task (ADR 0077's consequences).

## Decision

**An env template is not a secret.** A file named `.env.example`, `.env.sample` or `.env.template`, in any folder and letter case, is protected only by a glob that is its own path, never by `.env.*` or another wildcard. The real files, `.env`, `.env.local` and the rest, stay protected. The one matcher (`check_protected_paths`) serves the tool calls and the done gate alike. Claude Code's own `Read` deny rules still carry `.env.*`, since they cannot say "but not"; an agent writes a new template with its file tools and reads one with `catervas_exec`.

**Readiness refuses a protected allowed path** (`protected_path_allowed`): a task that lists a path no tool may write could never be done, so the Product Manager is told while it can still change the contract, not the owner after a dozen blocked sessions.

**An idle builder gets a re-plan.** When nothing can be assigned and a Software Developer or UI/UX Designer holds no task while two or more `ready` tasks of its role wait unheld in the lanes of an epic in progress, the Product Manager gets one `plan` session about the epic. It is told who is idle and each waiting task with its lane and dependencies. For each task that needs no unfinished task's result it moves the task back to `refining` (a new row of the table, the Product Manager's, for a task no one holds), rewrites its `lane`, `dependencies` and `allowed_paths` so that it is a lane of its own for an idle builder, and readiness runs again, `lanes_overlap` and `lane_beyond_builders` included. When the tasks truly wait on one another, it changes nothing. A process asks once for each epic, role and set of idle builders.

## Alternatives rejected

- **Let any free builder take a lane's next task.** A chain still runs one task at a time, and two builders in one lane could change the same paths at once, which lanes exist to prevent.
- **Start a task once its dependency is under review**, on the dependency's branch. It overlaps a review with the next task, but gives an idle second builder nothing in a chain.
- **Let the Architect rewrite the plan.** The Product Manager writes the contracts from the plan and is the one who may change them; the plan's lanes and dependencies are what it rewrites.
- **Let the owner waive a criterion** ("Skip C3"). A failing criterion is never skipped to get green; the contract is changed instead, which the Product Manager already can.
- **Let the team file remove a shipped protected glob.** The shipped list is the floor every team keeps; only the template, which holds no secret, leaves it.

## Consequences

- A task like CTV-11 writes its `.env.example`; a contract that lists a real secret file is sent back before it is ready.
- A builder added after an epic was planned gets work once the Product Manager re-plans the waiting tasks, which costs one plan session.
- Two builders on lanes the re-plan made still never touch the same paths.
- An agent that declares the same block again after the owner's answer changes nothing still starts a new session each time; ending that loop is left for later.
