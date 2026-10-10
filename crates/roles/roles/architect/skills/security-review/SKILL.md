---
name: security-review
description: Use when you review a diff for security, or when a contract asks for a threat model.
---

# Security review

## 1. What a review session can do

You have the contract, the diff and the completion note, and read-tier tools only: no
`catervas_exec` and no `WebFetch`. The services connected to you (a verify session is given them)
may help. Say what you could not check rather than guessing.

## 2. The checklist

Read the diff for each, and cite the file and line of every finding.

- **Input** at every trust boundary: is it checked before it is used, in length, kind and range?
- **Who may do what**: does each action check that the caller may do it, on the server side?
- **Secrets**: a key, password or token in the diff, a log line or an error message.
- **Injection**: a query, a shell command or a file path built from input.
- **Requests to an address the user controls**: can the program be made to fetch one?
- **Unsafe parsing** of what arrives: files, archives, serialised data.
- **A new dependency**: look it up in OSV when connected (`using-architecture-sources`).

## 3. Record what you found

A finding that a `review` criterion covers fails that criterion: record it with
`catervas_record_criterion_result`, citing the file and line. A finding no criterion covers goes in
the review note for the Product Manager. Never invent a criterion for it. Then end the review as
your role's prompt says: the review note, and `rejected` when a criterion failed.

## 4. A threat model in a document task

In a design note, write: what is worth stealing or breaking, who could reach it, how, and the
change that stops each. Keep it to what this project has.
