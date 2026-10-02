---
name: scoping-a-release
description: Use when choosing what ships together in a release, and what is left out.
---

# Scoping a release

A release is a promise to the user. Scope it so you can keep it.

## 1. Sort into three

- **Must**: the release is not worth shipping without it.
- **Should**: it matters, and it can wait one release if it has to.
- **Could**: nice to have; first to go.

Only work with a clear contract goes in the list.

## 2. Draw the cut line

Write down where the line is: everything above ships, everything below does not. Put the line
where the team's budget and the sprint's time let you keep the promise.

## 3. Say what is left out

State plainly what is not in the release and why, so the user is not surprised. A cut that is not
written down comes back as an argument.

## 4. The notes

Write the release notes from accepted tasks only, in the user's words: what changed for them. Do
not list work that is still open or was sent back. Save them with `farik_write_product_doc`.
