# 0012. The playbook is how Praeco learns

Date: 2026-10-24
Status: proposed

## Context

The founder wants the agent to "learn by itself". Options: fine-tune a model on the company's data (not offered for these models, and against Slack's terms for anything from Slack); let the agent write its own memory freely (Managed Agents memory stores allow it); or keep a playbook of written rules that Praeco writes from feedback through the governor. An agent that writes its own memory from what it reads on the web can be taught by anyone who can publish a page, which is prompt injection with a long life.

## Decision

Learning is a **playbook**: rules in plain words, at most 300 characters each and 100 active per workspace, each with the events that taught it (SPEC 5.7).

- Only feedback from approvers and owners, and measured results, teach. The web never does.
- A weekly `learning` cycle, and one after every ten feedback events, proposes rules with `propose_learning`. The governor applies a rule at once when two or more approvers' feedback events agree, and asks an approver otherwise.
- The playbook lives in Postgres (`playbook_rules`, versioned by events) and is written to the workspace's `playbook` memory store as one file, mounted read-only in every session. The agent cannot write it.
- The governor never reads the playbook. Learning changes what the agent writes and chooses, never what it may do.
- Every rule has an Undo; every application is announced in the digest.

## Consequences

"It learns" is something a company can read, argue with and undo, and it survives a model change.

What cuts against it: learning is slower than a model that adapts silently, and needs approvers to give feedback; phase 7 step 04's replay check measures whether the playbook actually raises the share of drafts approved without an edit, and the M1 gate measures it with real partners.
