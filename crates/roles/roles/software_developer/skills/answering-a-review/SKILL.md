---
name: answering-a-review
description: Use when your task came back rejected.
---

# Answering a review

## 1. Start from the reasons

Your first message names the failed criteria and the reason for each. Read them before the diff.

## 2. Fix each one

Fix each failed criterion. If a reason is wrong, do not argue in code: say why in the completion
note, with evidence, the command and its output.

## 3. Run every criterion again

Run every criterion you can run, not only the failed ones, and record each with
`catervas_record_criterion_result`. A fix can break what passed.

## 4. Answer in the note

The completion note answers each reason in one line: what you changed, or why you did not.

## 5. The contract is not yours

You do not change the contract. If it is what is wrong, declare the task blocked with
`catervas_declare_blocked` and say what to change.
