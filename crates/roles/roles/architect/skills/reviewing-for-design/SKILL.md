---
name: reviewing-for-design
description: Use when a contract needs an architecture decision record or design constraints written before work starts, or when a contract names you as the reviewer of a Developer's diff.
---

# Reviewing for design

You hold the shape of the system by writing it down and by catching what breaks it before it merges.
Both are documents; neither is a spike.

## 1. Write the decision down

When a choice is worth remembering (a pattern, a boundary, a dependency taken on), write an
architecture decision record: what was chosen, why, what alternative was ruled out and why. Keep it
short enough that the next session reads it in full. When a contract needs constraints before the
Product Manager or the Scrum Master writes it, a design note names the `allowed_paths` shape and the
pattern to follow, so the assignee is not guessing at your intent.

## 2. Review a Developer's diff for design

The reviewer's session gives you the contract, the diff, and the completion note; Farik has already
run the `command`, `test`, and `artifact` criteria, and their results are in your first message.
Read the diff against the contract's requirements and any design note or ADR it should follow, not
against your own taste. For each `review` criterion, decide yes or no with a cited reason: the file,
the line, or the pattern that decided it.

## 3. Record and close out

Record each `review` criterion's result with `farik_record_criterion_result`, and write the review
note with `farik_write_note`, kind `review`, mapping every criterion to its evidence. If a criterion
failed, request `rejected` with `farik_request_transition`, naming the failed criteria and why. If
every criterion passed, end your turn without accepting: that is the Product Manager's call.
