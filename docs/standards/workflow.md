# Workflow

How a change gets into the Farik repository, from idea to merged commit. This document governs how Farik is built. It is not the product's harness; that lives in `docs/SPEC.md` section 5 and governs the agent teams Farik runs for its users. The two are kept deliberately similar in spirit, and if a rule here would be embarrassing to apply to our users' teams it does not belong here either, but they are separate documents about separate things.

The process applies to humans and to AI agents alike.

## Vocabulary

Each of these words has exactly one meaning in this repository. Documents that need another meaning use another word.

| Word | Means | Defined in |
|---|---|---|
| Stage | One of the six parts of this workflow: brainstorm, plan, execute, verify, pull request and review, finish | this document |
| Phase | A body of product work in the project plan that ends in something usable; one branch, one pull request | `docs/plans/project-plan.md`, ADR 0003 |
| Step | A unit of work inside a phase with its own plan file; lands as commits on the phase branch | `docs/plans/project-plan.md`, `docs/plans/step-template.md` |
| Task | A unit inside a step plan with its own test cycle and its own commit | `docs/plans/step-template.md` |

The product has its own vocabulary (task contract, sprint, ceremony, governor, tier) in `docs/SPEC.md` section 13. "Task" there means a contracted unit of work for an agent team and is unrelated to a plan task here; the two never appear in the same document except the project plan, which says which it means.

The process is the one the [superpowers](https://github.com/obra/superpowers) plugin for Claude Code enforces (ADR 0001), with one exception: step plans are written by copying `docs/plans/step-template.md`, not with the plugin's `writing-plans` skill, which puts the implementation into the plan (ADR 0008, ADR 0010). Where the plugin and this document disagree, this document wins.

## The sequence

Every non-trivial change goes through these six stages in order. (Stages of the workflow, not to be confused with phases of the project plan, which are bodies of product work; see stage 2.) "Non-trivial" means anything that adds or changes behavior. A typo fix or a one-line doc correction skips to stage 5.

```
1. brainstorm  →  2. plan  →  3. execute (TDD, one task at a time)  →  4. verify  →  5. pull request and review  →  6. finish
```

Skipping a stage is allowed only when the person who owns the change says so in the pull request, in writing, with a reason.

### 1. Brainstorm

Before any code or plan, refine the idea by asking questions. What is the user-facing outcome? Which section of `docs/SPEC.md` does this serve, and which functional requirement number? What is out of scope? What could be simpler? Present the design in small sections and get agreement on each before moving on.

Output: a short design, two to twenty lines, that becomes the header of the plan. For a change that reverses or adds to a decision in the spec, also an ADR (see `docs/decisions/`).

### 2. Plan

Planning has two levels, and both are written before any product code.

The project plan, `docs/plans/project-plan.md`, divides the whole project into phases and each phase into steps. A phase is a body of work that ends in something a person can use or verify (a command-line harness, a running desktop shell). A step is a self-contained unit of work inside a phase, executed from its own plan and landed as a group of commits on the phase branch. A phase is one pull request. The project plan lists every phase and step in order, one line each, with the decisions each phase depends on and whether each decision is made. A step is not started until every decision its phase depends on is recorded as made, in the project plan or in an ADR.

The step plan, one file per step at `docs/plans/phase-<n>-<name>/step-<nn>-<name>.md`, is written from `docs/plans/step-template.md` for a skilled developer who knows nothing about this codebase or the problem domain. Its header carries the goal, the spec reference (section and F-number), the decisions it rests on, the steps it depends on, the file map, and the interfaces the step consumes and produces. Then tasks. CLAUDE.md's hard rules apply to every plan and are not restated in it.

A plan states what has been decided and what the boundaries are; it does not contain the implementation. Signatures yes, function bodies no. Test names and what each asserts yes, test code no. Exact file paths yes, and expected command output yes, because that is the verification and nothing else checks it. A plan carrying its own implementation is the codebase written twice, and the copy in Markdown is the one no compiler can read; ADR 0008 records the measurements that settled this. A plan past roughly 300 lines is usually a step that wants splitting, or a plan that has started writing the code.

A task is the smallest unit that carries its own test cycle and deserves a fresh reviewer's look. Each task lists the exact files it creates, modifies, and tests; the interfaces it consumes from earlier tasks and produces for later ones; the tests to write, each named and each with what it asserts; and one checkbox, ticked by the task's commit. Map the file structure before writing tasks so that two tasks never fight over one file, and state every type and signature the step produces or consumes, which is what the no-forward-dependency rule is checked against.

Three rules decide whether a plan is ready to execute. A reviewer other than the author checks all three before the first task starts, and a plan that fails any of them goes back to brainstorming.

That review is one round. A second round happens only when the first finds a decision the plan rests on that has not been made — the one failure that execution cannot recover from, because an undecided question gets decided silently and differently by whoever hits it next. Everything else the reviewer finds is written down and carried into execution. Rounds three and beyond are a sign that the plan is being proof-read rather than reviewed, which is work the compiler does better and faster.

Every decision is made. Features, enhancements, names, data shapes, library choices, error behavior: all decided and written down in the plan's Decisions section or in an ADR it links. A decision that affects more than one step is an ADR.

No ambiguity, in what the plan is for: the decisions, the boundaries, the interfaces. Every file path is exact. Every signature is exact. Every named test says what it asserts, specifically enough that two people would write the same assertion. Every command has its expected output. Words like "appropriate", "as needed", "and so on", "TBD", and "similar to" fail review.

Ambiguity inside an implementation is not this gate's job. A type that does not line up, a call with the wrong arity, a case left unhandled: `cargo check` finds those in seconds and a reader finds them in an hour, if at all. The plan fixes what the compiler cannot know — what was decided and why — and the compiler and a test watched to fail take the rest.

No forward dependencies. A step depends only on phases already merged to `main` and on earlier steps already committed on the same phase branch. A task depends only on earlier tasks in the same plan. Nothing in a plan stubs, mocks, or leaves a placeholder for work that a later step will do; if a later step needs an interface, the later step adds it. Phases are ordered so that this holds across the whole project plan, and a step whose plan cannot be written without a forward reference means the phase is in the wrong order.

### 3. Execute

Work on the phase branch, `phase/<n>-<name>`, created from `main` when the phase's first step starts and kept until the phase merges. Use a git worktree for it so that a clean test baseline can be confirmed before the first change. Run the full check once at the start of the phase, and again at the start of a step only when the branch's head is not the commit the previous step's landing review verified.

Then one task at a time, in plan order, each under test-driven development:

RED. Write one minimal failing test that shows the desired behavior. Run it. Confirm it fails because the behavior is missing, not because of a typo or a missing import.

GREEN. Write the simplest code that makes the test pass. No extra features, no refactoring of neighboring code, no generality that the test did not ask for. Run the test and the rest of the suite.

REFACTOR. With everything green, remove duplication, improve names, extract helpers. Do not add behavior. Keep the suite green.

Commit after each task with a message per `code.md`. Tick the task's checkbox in the plan as part of the same commit.

Code written before its test is deleted. Not kept as reference, not adapted, not consulted. The one exception is explicit permission from the change owner, recorded in the plan.

Tasks may be dispatched to fresh subagents. They are not reviewed one by one: the step's landing review (stage 5) is the review. A subagent's report that it succeeded is not evidence; the landing reviewer runs the checks.

### 4. Verify

Before any claim that a task, a plan, or a pull request is done, run the command that proves it, freshly, and read the whole output and exit code. Evidence is that command and its output, pasted into the pull request; "should work", "seems to work", or an agent's paraphrase of its own success is not. The plugin's `verification-before-completion` skill implements this.

The full check is `cargo xtask check` once the workspace exists (format check, clippy, tests, the bare-TODO check, the core no-I/O check, and the front end's checks once it exists). Until then there is no check command, and the verification section of a pull request says so explicitly.

### 5. Pull request and review

Self-review first, against the plan, before requesting anyone else's time. Read the diff as a hostile reviewer would: what would make CI reject this, what did the plan ask for that is missing, what is here that the plan did not ask for.

Then open the phase's pull request to `main` using the template. One phase is one pull request, opened as a draft when the phase's first step is pushed and marked ready for review when the last step's verification passes. This is not optional and it is not deferred: a pushed phase branch with no pull request is unfinished work that nobody can see. The request explains what changed and, above all, why it was necessary: what problem or spec requirement the phase serves and what would be wrong without it. It also carries links to every step plan in the phase, the spec references, the verification evidence from the final commit, and any ADRs.

A pull request never contains work from two phases. Each step inside a phase is still reviewed as it lands: the reviewer reads the step's commits against its plan on the phase branch and records the review in the pull request thread, so that the final review of the whole phase is a confirmation rather than a first reading.

This landing review is where the defects are, and it is not optional: a step whose landing review did not complete is reviewed before the pull request is marked ready. It reads running code, not a plan, and its acceptance bar is mutation: re-introduce the bug each test claims to catch and confirm the suite notices. A test that passes both with the code and with the code broken is not a test, and a green check does not distinguish them. ADR 0008 records what these reviews found that no plan review could.

Reviewers report findings by severity: critical (blocks merge: correctness, security, a rule in this document broken), important (must be addressed or explicitly deferred with a reason), and minor (author's call). A critical finding is never resolved by a comment; it is resolved by a commit.

Receiving review: address each finding or reply with a reason; never resolve a thread silently. Push fixes as new commits, not force-pushes, so the reviewer can see what changed.

### 6. Finish

When the phase's pull request is green and approved: confirm the full check once more on the final commit, merge into `main` with a merge commit so that every task commit survives in history, delete the phase branch. The merge commit message follows `code.md` and links the project plan.

If the work is abandoned, say so on the pull request and close it. Do not leave branches open without a note.

## Definition of Done for a change

The checklist in `.github/pull_request_template.md`, which mirrors section 5.4 of the spec on purpose. A pull request needs a review from someone other than its author, or, for a solo maintainer, a fresh-session review by an agent that did not write the code.

## Debugging

Follow the plugin's `systematic-debugging` skill. A fix without a reproducing test is a guess.

## What this workflow costs

It is slower per change than editing and hoping. That is the point, and it is also a real cost. Expect the first few changes to feel heavy while templates and habits settle. If a stage consistently produces nothing of value for a class of change, propose removing it for that class through an ADR rather than skipping it quietly.

ADR 0008 and ADR 0010 are the times it has been used.
