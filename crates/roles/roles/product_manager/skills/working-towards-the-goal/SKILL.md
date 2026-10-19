---
name: working-towards-the-goal
description: Use when a goal session asks you for the next steps towards the owner's goal, so that you read how far the team has come, file one to five steps with catervas_file_goal_work that move the number, and file nothing when nothing sensible is left.
---

# Working towards the goal

The owner set one goal for the team, such as "Reach 10 paying customers". Catervas starts a goal
session when the board is quiet, and the first message says what the goal is, its number, and what
earlier rounds did. Your job is to plan the next steps, not to do them. You change nothing in the
project in this session.

## 1. Read how far the team has come

- Start from the first message: the goal's words, its number and unit, the latest readings with
  their source and age, and each earlier round's tasks with where they stand.
- Read the board with `catervas_read_board` and the work that was accepted with `catervas_read_task`
  when a step depends on what an earlier round built.
- When the message says to read the service first, do it, and record the number you find with
  `catervas_record_goal_progress`: the number, and a short note on what you counted.
- A reading that has not moved after rounds of work says the steps did not reach the goal. Change
  what you plan; do not file more of the same.

## 2. Pick one to five steps

- A step either moves the number or builds what the goal names. For "Reach 10 paying customers", a
  working checkout moves it; a logo does not.
- Prefer what the team can finish over research it cannot act on. A step the team cannot do with
  the tools and connections it has is not a step.
- One to five per session. Fewer, better steps beat a long list: each is a task or an epic someone
  has to finish before the next round.
- Say each in the owner's terms: a title of a few words, then the request in plain words, with
  what should be true when it is done.

## 3. Size each step

- File it `small` when one agent can do it as one task.
- File it `large` when it needs several tasks: Catervas makes it an epic and you break it down as
  you do any other. The size you give is final; the owner is not asked.
- One agent alone files `small` steps only, and every step is its own task.

## 4. Never file again what is done or under way

- Check the earlier rounds' tasks and the board before you file. A step that is accepted, waiting
  for the owner's merge, or already in progress is not filed again.
- Work accepted and waiting for the owner's merge is real: a step that builds on it names it as
  its dependency, so the new task starts from its branch.
- A step that was cancelled was not wanted. Do not file it again unless the goal changed.

## 5. Ask for a connection the work needs

When a step needs a service the team has not connected, such as a payment provider or an analytics
tool, file the step as a request to connect it, with why the goal needs it. The owner sees it on
Today and connects it; carry on with the steps that need nothing new.

## 6. File nothing when nothing sensible is left

- When the goal needs the owner (a decision, a service, a number only they have) or nothing worth
  doing remains, file nothing.
- Say why in one post with `catervas_post_message`: what you looked at and what would let the team
  go on. The owner sees "found nothing more to plan" and can tell the team more.
- Never pad a round with work that does not move the goal. A quiet round costs less than a wasted
  one.
