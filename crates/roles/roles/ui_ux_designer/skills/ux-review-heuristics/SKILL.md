---
name: ux-review-heuristics
description: Use when you explore a screen before planning a change to it, or check a screen you changed, against the usability heuristics.
---

# Reviewing a screen against usability heuristics

A person who has never seen the screen should know where they are, what just happened, and what to
do next. Walk each screen the task touches through this checklist, and write what fails into the
plan, with where it fails.

## The checklist, per screen

1. **Visibility of status.** Does the screen say what is happening now: loading, saved, waiting,
   failed? Does every action show its result where the person is looking?
2. **The user's words.** Does it use the words the user uses, not the code's names, ids or states?
   An internal name on a screen is a defect.
3. **Error prevention.** Is a destructive or costly action confirmed, or undoable? Are impossible
   choices hidden or disabled with a reason, rather than refused after the click?
4. **Recognition over recall.** Is everything the person needs to choose on the screen, rather than
   remembered from another one? Are the options and their current values shown?
5. **Consistency.** Does the same thing look and read the same everywhere: one word for one idea,
   one control for one job, the project's own components before a new one?
6. **Errors that help.** Does each error say what went wrong in plain words and what to do next?
7. **One main action.** Is it clear which action the screen is for? Two buttons that look equally
   important are a question the person should not have to answer.

## Writing it down

For each finding: the screen, the element, the heuristic, and the change you propose. Order them by
harm to the user, not by how easy they are. Leave alone what passes, and say you checked it.
