---
name: catervafying-the-repository
description: Use when a task asks you to write the project's UI inventory, a task of its Catervafication.
---

# Catervafying the repository

The Catervafication is the first thing the team does in a project: each role reads the project and
writes down what it knows, in its own folder, before anything is built. Yours is
`docs/catervas/design/`.

## 1. What the task asks

Write `ui-inventory.md`: every screen or page the project has, and the parts the interface is made
of. It is for agents. Touch nothing outside `docs/catervas/design/`, and change no code.

## 2. Read first

Read the code that draws the interface: routes and pages, components, styles and design tokens,
strings files and the project's own design notes. With the preview running and your browser
available, look at the screens as well; what you see there is evidence too.

## 3. What the inventory holds

- The screens, one section each: its route or file, what it is for, and the components it uses.
- The shared components, with where they live and where they are used.
- The design tokens and styles: colours, type, spacing, and the file that defines them.
- The words the interface uses for its main actions, and where they are kept.
- What the project does not have, such as a missing phone layout or no design tokens, said plainly.

## 4. Write only what you can show

Write each entry with where you read it: a file path, or the screen you looked at. Never guess at
a screen you did not find.

## 5. Finish

Commit with `catervas_git_commit`, and request `verifying`. The reviewer checks the inventory
against the code.
