---
name: writing-escalation-digests
description: Use when escalations are open at planning, to give the human one short list of what they must decide.
---

# Writing escalation digests

An escalation is a question only the human can answer. The digest is the list of them.

## 1. Gather the open ones

Read the board (`farik_read_board`) for open escalations. Leave out any already answered.

## 2. Order them

Oldest first. The oldest has waited longest and may block the most.

## 3. One line each

For each, say what the human must decide, in one line, and since when. Do not retell the history.
If the decision needs context, link the task.

## 4. Send it

Post the digest with `farik_post_message`. Nothing already answered goes in it.
