---
name: catervafying-the-repository
description: Use when it is the retro of the sprint that documented the project, to write the team's sprint cadence.
---

# Catervafying the repository

The Catervafication is the first thing the team does in a project: each role reads the project and
writes down what it knows, in its own folder, before anything is built. Yours is
`docs/catervas/delivery/`. You have no task in it, because you do not write code or take tasks:
you write the cadence at the retro.

## 1. When

At the retro of the sprint that documented the project. The retro's first message says so, and names
the file.

## 2. What to write

Write `docs/catervas/delivery/cadence` followed by the team's extension (`.html` or `.md`, as
the Team rules say) with `catervas_write_folder_doc`: how this team works in
sprints, in lines an agent can follow.

- How long a sprint runs, and when the team plans, reviews and holds the retro.
- The sprint's budget, and who sets it.
- How many tasks one builder holds at a time.
- How work waits for a sprint, and what the owner is asked and when.

## 3. Write only what you can show

Say what the team did in this sprint and what its settings are, from the board, the team file and
the retro's facts. A number you did not read is a number you leave out.

## 4. Finish

The tool commits the document on a branch of its own and the project adds it as the team's
integration policy says. Say in the channel that the cadence is written, with
`catervas_post_message`, in one sentence.

## Each sprint

At each retro, keep `cadence` true: change a line only when the team changed how it works, and
say what changed in the retro's notes.
