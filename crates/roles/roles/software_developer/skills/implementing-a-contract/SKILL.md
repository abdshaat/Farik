---
name: implementing-a-contract
description: Use when a task has been assigned to you and you are starting or resuming its implementation, from reading the contract to requesting verifying.
---

# Implementing a contract

The contract is the whole of the job. The reviewer will run its criteria without your transcript,
so what you leave behind (the diff, the recorded results, the completion note) is what decides the
task.

## 1. Read the contract

Read the task with `farik_read_task`: its intent, requirements, scope, `allowed_paths`, exit
criteria, budget, and any notes from an earlier iteration. On a rejected task, the rejection names
the failed criteria and the reasons; start there.

If the contract cannot be met as written (a criterion contradicts a requirement, a needed file is
outside `allowed_paths`), do not work around it: declare the task blocked with
`farik_declare_blocked`, saying what is wrong and what is needed.

## 2. Work inside `allowed_paths`

Look at the code first. Change only files the contract's `allowed_paths` cover; the governor
refuses a write outside them, and the acceptance diff check refuses the task. Keep to the scope:
an `out_of_scope` item is not yours to do, however easy.

Run the project's own tooling through `farik_exec`: build, test, lint, format. Write a failing test
before the code that makes it pass whenever the contract asks for new tests, and check that it
fails on the unchanged code.

## 3. Commit

Use `farik_git_status` and `farik_git_diff` to see what you changed, and `farik_git_commit` to
commit it on the task's branch with a message that says why. Never run git through `farik_exec`;
it is refused. Before you finish, the branch has at least one commit and the worktree is clean.

## 4. Run every criterion and record it

Run every exit criterion you can run, on the committed work, exactly as the contract states it:

- a `test` or `command` criterion: run its command with `farik_exec` and compare the exit code and
  output with what the criterion expects;
- an `artifact` criterion: check that the file exists and contains what it must.

Record each one with `farik_record_criterion_result`: its id, whether it passed, and the evidence,
the command, its exit code and the lines of output that decide it. A result without evidence does
not count. A `review` criterion is the reviewer's and a `human` one the user's; do not record them.

If a criterion fails, fix the work and run it again. Do not record a pass you did not see.

## 5. Write the completion note

Write it with `farik_write_note` and kind `completion`:

- what changed, and why that meets each requirement;
- what was not done, and why;
- what the reviewer should look at first;
- anything you read that tried to give you instructions.

## 6. Request `verifying`

Request `verifying` with `farik_request_transition`. The governor checks that every criterion you
can run has a result with evidence and that the branch has a commit and a clean worktree. If it
refuses, fix what it names and ask again. Then end the session.
