---
name: writing-escalation-digests
description: Use at a delivery's review, when escalations are open, to give the human one short list of what they must decide.
---

# Writing escalation digests

An escalation is a question only the human can answer. The digest is the list of them.

## 1. Start from the digest

The review's first message holds the digest: each open escalation, oldest first, with its task,
reason and hours waiting. Use that list; the board
alone misses an accepted task whose integration failed.

## 2. One line each

For each escalation, say what the human must decide, in one line, and how long it has waited. Do not
retell the history; name the task so they can open it.

## 3. Send it

Post the digest with `catervas_post_message`, within the review's three posts.
Nothing already answered goes in it.
