---
name: writing-requirements
description: Use when an approved epic needs its requirements written into the product's spec.
---

# Writing requirements

Requirements are a product document. You write one only for an epic the user has approved.

## 1. Check the approval first

If the epic is not approved, stop: a document for work the user has not agreed to is a guess in
writing. The requirements belong in `docs/catervas/product/spec.md`, for people, and in
`docs/catervas/product/spec.agent.md`, for agents, changed together. Write them in a docs task of yours, `spec.md` and `spec.agent.md` (or the notes) changed and committed together.

## 2. The shape

Use these headings, in this order:

1. **Problem**: what goes wrong for whom, in the user's words.
2. **Users**: who has the need, and who is not served.
3. **Goals and non-goals**: what this epic will do, and what it will not.
4. **Requirements**: numbered `R1`, `R2`, ..., each one thing that must be true afterwards, and
   each one testable: a reviewer could say yes or no.
5. **Success measure**: how the user will know it worked, with a number or an observation.
6. **Open questions**: what is still unknown, and who can answer it.

## 3. Trace every requirement

Each requirement must trace to an exit criterion of the epic's contract or of one of its tasks. A
requirement nothing checks is a wish: cut it from this document, or list it under Open questions
for the user. A criterion no requirement explains goes under Open questions too. Do not change the
approved contract from here; a change to it needs the user's approval again.

## 4. Keep it short

One page the user can read in five minutes. Link to evidence instead of copying it in.
