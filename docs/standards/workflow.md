# Workflow

How a change gets into Farik, from idea to merged commit. This applies to humans and to AI agents alike, and it applies to Farik's own code exactly as strictly as Farik's harness applies to the projects it manages. If a rule here would be embarrassing to apply to our users' teams, it does not belong here either.

The process is the one enforced by the [superpowers](https://github.com/obra/superpowers) plugin for Claude Code, adopted as-is with Farik-specific paths and a few additions. Contributors using Claude Code should install the plugin; its skills trigger automatically. Contributors working by hand follow the same steps manually. ADR 0001 records the adoption.

## The sequence

Every non-trivial change goes through these phases in order. "Non-trivial" means anything that adds or changes behavior. A typo fix or a one-line doc correction skips to phase 5.

```
1. brainstorm  →  2. plan  →  3. execute (TDD, one task at a time)  →  4. verify  →  5. pull request and review  →  6. finish
```

Skipping a phase is allowed only when the person who owns the change says so in the pull request, in writing, with a reason.

### 1. Brainstorm

Before any code or plan, refine the idea by asking questions. What is the user-facing outcome? Which section of `docs/SPEC.md` does this serve, and which functional requirement number? What is out of scope? What could be simpler? Present the design in small sections and get agreement on each before moving on.

Output: a short design, two to twenty lines, that becomes the header of the plan. For a change that reverses or adds to a decision in the spec, also an ADR (see `docs/decisions/`).

### 2. Plan

Planning has two levels, and both are written before any product code.

The project plan, `docs/plans/project-plan.md`, divides the whole project into phases and each phase into steps. A phase is a body of work that ends in something a person can use or verify (a command-line harness, a running desktop shell). A step is one pull request's worth of work inside a phase. The project plan lists every phase and step in order, one line each, with the decisions each phase depends on and whether each decision is made. A step is not started until every decision its phase depends on is recorded as made, in the project plan or in an ADR.

The step plan, one file per step at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`, is written from `docs/plans/step-template.md` for a skilled developer who knows nothing about this codebase or the problem domain. Its header carries the goal, the spec reference (section and F-number), the decisions it rests on, the steps it depends on, the architecture notes, and the global constraints. Then tasks.

A task is the smallest unit that carries its own test cycle and deserves a fresh reviewer's look. Each task lists the exact files it creates, modifies, and tests; the interfaces it consumes from earlier tasks and produces for later ones; the actual code, not "add validation here"; the exact commands to run with expected output; and a checkbox per step. Steps inside a task are two to five minutes each: write the failing test, watch it fail, write the minimal code, watch it pass, commit. Repeat code across tasks rather than referencing an earlier task. Define every type and signature inside the plan. Map the file structure before writing tasks so that two tasks never fight over one file.

Three rules decide whether a plan is ready to execute. A reviewer checks all three before the first task starts, and a plan that fails any of them goes back to brainstorming.

Every decision is made. Features, enhancements, names, data shapes, library choices, error behavior: all decided and written down in the plan's Decisions section or in an ADR it links. A decision that affects more than one step is an ADR. The Open questions section of a ready plan says "none".

No ambiguity. Every file path is exact. Every code block is the code that will be written. Every command has its expected output. Words like "appropriate", "as needed", "and so on", "TBD", and "similar to" fail review, as does any task whose steps a second reader could carry out differently from the first.

No forward dependencies. A step depends only on steps that are already merged to `main`. A task depends only on earlier tasks in the same plan. Nothing in a plan stubs, mocks, or leaves a placeholder for work that a later step will do; if a later step needs an interface, the later step adds it. Phases are ordered so that this holds across the whole project plan, and a step whose plan cannot be written without a forward reference means the phase is in the wrong order.

### 3. Execute

Work on a branch named per `code.md`, ideally in its own git worktree so that a clean test baseline can be confirmed before the first change. Confirm that baseline: run the full check and record that it passed before touching anything.

Then one task at a time, in plan order, each under test-driven development:

RED. Write one minimal failing test that shows the desired behavior. Run it. Confirm it fails because the behavior is missing, not because of a typo or a missing import.

GREEN. Write the simplest code that makes the test pass. No extra features, no refactoring of neighboring code, no generality that the test did not ask for. Run the test and the rest of the suite.

REFACTOR. With everything green, remove duplication, improve names, extract helpers. Do not add behavior. Keep the suite green.

Commit after each task with a message per `code.md`. Tick the task's checkboxes in the plan as part of the same commit.

Code written before its test is deleted. Not kept as reference, not adapted, not consulted. The one exception is explicit permission from the change owner, recorded in the plan.

For larger plans, dispatch a fresh subagent per task and review each result in two stages: first against the plan (did it do what the task said, nothing more, nothing less), then for code quality. A subagent's report that it succeeded is not evidence; the reviewer runs the checks.

### 4. Verify

Before any claim that a task, a plan, or a pull request is done:

1. Identify the command that would prove the claim.
2. Run it, completely and freshly, not from memory of an earlier run.
3. Read the whole output and the exit code.
4. Confirm the output actually supports the claim.
5. Only then state the claim, with the evidence.

Phrases that are not allowed in a completion claim: "should work", "probably passes", "seems to work", "I'm confident", "done" without output. A paraphrased success report from an agent or a subagent is not evidence either. Evidence is the command and its output, pasted into the pull request.

The full check is `pnpm check` once the monorepo exists (it runs typecheck, lint, format check, and tests). Until then there is no check command, and the verification section of a pull request says so explicitly.

### 5. Pull request and review

Self-review first, against the plan, before requesting anyone else's time. Read the diff as a hostile reviewer would: what would make CI reject this, what did the plan ask for that is missing, what is here that the plan did not ask for.

Then open a pull request to `main` using the template. This is not optional and it is not deferred: the moment a unit of work is complete and pushed, its pull request exists. A pushed branch with no pull request is unfinished work that nobody can see. The request explains what changed and, above all, why it was necessary: what problem or spec requirement it serves and what would be wrong without it. It also carries the plan link, the spec reference, the verification evidence, and any ADRs.

One plan is normally one pull request. A large plan may be split into several, each self-contained and each mergeable on its own; the plan says where the splits are. A pull request never contains work from two plans.

Reviewers report findings by severity: critical (blocks merge: correctness, security, a rule in this document broken), important (must be addressed or explicitly deferred with a reason), and minor (author's call). A critical finding is never resolved by a comment; it is resolved by a commit.

Receiving review: address each finding or reply with a reason; never resolve a thread silently. Push fixes as new commits, not force-pushes, so the reviewer can see what changed.

### 6. Finish

When the pull request is green and approved: confirm the full check once more on the final commit, squash-merge into `main`, delete the branch. The squash commit message follows `code.md` and links the plan.

If the work is abandoned, say so on the pull request and close it. Do not leave branches open without a note.

## Definition of Done for a change

Mirrors section 5.4 of the spec on purpose.

- Every task in the plan has its checkboxes ticked and a commit.
- The full check passes on the final commit, and the output is in the pull request.
- New behavior has tests that were watched to fail before they passed.
- No file outside the plan's stated file list changed. If one had to, the plan was updated first and the reason is in the pull request.
- Documentation that describes the changed behavior is updated in the same pull request. This includes `docs/SPEC.md` when the spec is what changed.
- Any decision that a future contributor would want to know the reason for has an ADR.
- The author has not accepted their own work. A pull request needs a review from someone other than its author, or, for a solo maintainer, a fresh-session review by an agent that did not write the code.

## Debugging

When something is broken, resist the urge to try fixes. Follow the four phases from the plugin's systematic-debugging skill: reproduce reliably, then trace to the root cause with evidence, then fix the cause rather than the symptom, then add the test that would have caught it. A fix without a reproducing test is a guess.

## What this workflow costs

It is slower per change than editing and hoping. That is the point, and it is also a real cost. Expect the first few changes to feel heavy while templates and habits settle. If a phase consistently produces nothing of value for a class of change, propose removing it for that class through an ADR rather than skipping it quietly.
