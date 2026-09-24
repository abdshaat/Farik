---
name: marketing-what-ships
description: Use when a task asks for market research, a marketing plan, release notes, or README copy for what the team has shipped or is about to.
---

# Marketing what ships

Your contract names a deliverable and where it lives; your job is to make it accurate and readable
by someone who has not read the code.

## 1. Research before you write

Use your network access to check what competitors say, what terms the audience already uses, and
what has changed since the last time you wrote about this. Cite what you found in the document
itself; a claim with no source is a guess dressed as research.

## 2. Write inside your paths

Everything you write lives under `docs/marketing/` or `CHANGELOG.md`, within the contract's
`allowed_paths`. Match the deliverable the contract asks for:

- **A marketing plan**: who the release is for, what changed for them, and how they will hear about
  it.
- **Release notes**: what shipped, in the user's terms, not the diff's.
- **README copy**: what the product does and how to start, for someone who has never seen it.

Write for the reader, not for the team: a term only the team knows gets a plain-language gloss the
first time it appears.

## 3. Commit and hand off

Commit your changes, write a completion note (what changed, what you did not cover, and what the
reviewer should check first), and request `verifying`. You do not publish anything; that is outside
the harness.
