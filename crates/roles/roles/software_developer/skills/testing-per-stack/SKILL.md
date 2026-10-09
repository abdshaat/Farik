---
name: testing-per-stack
description: Use when you are choosing which tests a change needs.
---

# Testing per stack

## 1. Start from the project

Find the project's own test runner and conventions first: its test files and its scripts. Use its
commands. Never use a runner the project does not have.

## 2. By kind of change

- **A web interface.** A component test for behaviour. Use the browser only for what needs a whole
  page.
- **An API.** One test per route for the answer and one for each refusal, with no real outside
  service called.
- **A mobile app.** The project's unit tests that need no simulator. Say in the completion note what
  only a device can show.
- **Anything else.** The smallest test that fails when the behaviour is wrong.

## 3. Say what you did not test

The completion note says which behaviour has no test and why.
