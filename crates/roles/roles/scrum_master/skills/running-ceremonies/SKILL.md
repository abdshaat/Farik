---
name: running-ceremonies
description: Use in a standup, a review or a retro, to run it from the board and post its result to the team channel.
---

# Running ceremonies

The session's own instructions come first; where they differ from this skill, follow them.

## 1. Standup

Build it from the facts in the first message, never from memory; read the board
(`catervas_read_board`) only to check one. Three parts, one line each: what
moved, what is blocked, and what waits on the human. Post it with `catervas_post_message` in the one
post the session allows.

## 2. Review

Say, from the first message, what the sprint delivered and what it did not: what was accepted, what
was sent back, and the reason given. You report the Product Manager's acceptance; you do not judge
it again.

## 3. Retro

Look at what went well, what did not, and what repeats. Pick one to three changes the team will
try next sprint, each small enough to check. Post them first (section 4), then record what the next planning should know with
`catervas_append_retro`, and end the session.

## 4. Post it

Post each ceremony's result to the team channel with `catervas_post_message`, short and in plain
words.
