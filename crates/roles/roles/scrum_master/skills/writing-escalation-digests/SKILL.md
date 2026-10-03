---
name: writing-escalation-digests
description: Use when escalations are open at planning, to give the human one short list of what they must decide.
---

# Writing escalation digests

An escalation is a question only the human can answer. The digest is the list of them.

## 1. Start from the digest

The planning's first message holds the digest: each open escalation, oldest first, with its task,
reason and hours waiting, and each budget spent since the last planning. Use that list; the board
alone misses an accepted task whose integration failed.

## 2. One line each

For each escalation, say what the human must decide, in one line, and how long it has waited. Do not
retell the history; name the task so they can open it. Then one line for each budget spent.

## 3. Send it

Post the digest with `farik_post_message`, beside the plan and within the planning's three posts.
Nothing already answered goes in it.
