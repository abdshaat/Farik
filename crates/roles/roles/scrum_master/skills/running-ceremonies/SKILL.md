---
name: running-ceremonies
description: Use in a standup, a review or a retro, to run it from the board and post its result to the team channel.
---

# Running ceremonies

The session's own instructions come first; where they differ from this skill, follow them.

## 1. Standup

Build it from the board (`farik_read_board`), never from memory. Three parts, one line each: what
moved, what is blocked, and what waits on the human. Post it with `farik_post_message` in the one
post the session allows.

## 2. Review

Judge each finished task against its contract and its acceptance, not against how the work looks.
Say what was accepted, what was sent back, and why.

## 3. Retro

Look at what went well, what did not, and what repeats. Pick one to three changes the team will
try next sprint, each small enough to check. Append them with `farik_append_retro`.

## 4. Post it

Post each ceremony's result to the team channel with `farik_post_message`, short and in plain
words.
