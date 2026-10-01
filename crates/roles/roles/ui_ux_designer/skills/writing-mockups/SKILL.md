---
name: writing-mockups
description: Use when you write a design plan and need to show the screens you will change, as annotated wireframes and screen descriptions a Product Manager can approve.
---

# Writing a plan's mockups

The Product Manager approves your plan from its text alone. The mockups in it must let them see
the screen before it exists, and decide.

## 1. One block per screen

For each screen you will change, write:

- **Where it is**: the page, and how a person gets there.
- **Before**: what is there now, in one or two lines.
- **After**: a text wireframe, top to bottom, one line per element: its kind (heading, text, list,
  button, field), its words exactly as they will read, and its state.
- **Why**: which finding or requirement each change answers.

A text wireframe is enough; keep it plain and in reading order. For example:

```
[heading]  Your team
[text]     Six agents work on this project.
[list]     one card per agent: picture, name, role tag, one line
[button]   Add an agent   (main action)
```

## 2. Sizes and states

Say what changes at phone width (360 px) and at desktop width (1280 px), and in the dark theme when
it differs. Name the states the screen can be in (empty, loading, full, error) and show each one
that changes.

## 3. What you leave alone

End with what you will not change, so the Product Manager knows where the work stops.
