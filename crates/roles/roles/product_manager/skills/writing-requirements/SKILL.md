---
name: writing-requirements
description: Use when an approved epic needs its requirements written under .farik/product/.
---

# Writing requirements

Requirements are a product document. You write one only for an epic the user has approved.

## 1. Check the approval first

If the epic is not approved, stop: the governor refuses the write, and a document for work the user
has not agreed to is a guess in writing. Write it with `farik_write_product_doc`, under
`.farik/product/`.

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
requirement nothing checks is a wish; add the criterion or cut the requirement. A criterion that
no requirement explains is out of scope; say so or remove it.

## 4. Keep it short

One page the user can read in five minutes. Link to evidence instead of copying it in.
