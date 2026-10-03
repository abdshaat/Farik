---
name: test-driven-development
description: Use when you change behaviour, so that every change starts from a test you watched fail.
---

# Test-driven development

`implementing-a-contract` says where the work goes and how it is recorded; this is how you write
the code inside it.

## 1. The loop

1. Write one small test that shows the behaviour the contract asks for.
2. Run it with `farik_exec` and watch it fail. Read why: it must fail because the behaviour is
   missing, not because of a typing mistake or a missing import.
3. Write the least code that makes it pass. Nothing the test does not ask for.
4. Run the test and the rest of the project's tests. Then tidy the code with the tests green.
5. Commit with `farik_git_commit` when the tests are green.

A test that passes the first time it runs tests nothing yet. Change it until it fails for the
right reason, or the behaviour already exists and you say so.

## 2. When a criterion asks for new tests

A `test` criterion with `new_tests_required` is also run by Farik on the base branch, without your
change, and the new tests must fail there. A test that passes without your change fails the
criterion, so watch each new test fail before you write the code.

## 3. What you never do

- Never skip, delete or weaken a test to get green. A test that is wrong is fixed and the
  completion note says why.
- Never record a pass you did not see: the evidence is the command and the lines of output that
  decide it.
