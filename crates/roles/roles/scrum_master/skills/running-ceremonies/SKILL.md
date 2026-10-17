---
name: running-ceremonies
description: Use in a delivery's review or retro, to run it from the board and post its result to the team channel.
---

# Running ceremonies

The session's own instructions come first; where they differ from this skill, follow them.

## 1. Review

Say, from the first message, what the delivery delivered and what it did not: what was accepted,
what was sent back, and the reason given. You report the Product Manager's acceptance; you do not
judge it again. Read the board (`catervas_read_board`) only to check one fact.

## 2. Retro

Look at what went well, what did not, and what repeats. Pick one to three changes the team will
try in the next delivery, each small enough to check. Post them first (section 3), then record what
the next delivery should know with `catervas_append_retro`, and end the session.

## 3. Post it

Post each ceremony's result to the team channel with `catervas_post_message`, short and in plain
words.
