---
name: asking-the-right-questions
description: Use when a request is unclear, before writing its contract, so that you ask the user only what would change the contract.
---

# Asking the right questions

A question costs the user's time. Ask one only when its answer would change what you write.

## 1. Check what you already know

Read the request, the board (`farik_read_board`) and the decisions (`farik_read_decisions`) first.
Do not ask what they already answer.

## 2. The five kinds of question

Ask about one of these, and nothing else:

- **Who it is for**: the person who has the need.
- **The problem**: what goes wrong for them today.
- **How we will know it worked**: something the user could see or measure.
- **What is out**: what the work must not touch or change.
- **Limits**: dates, money, or anything the user cannot give way on.

## 3. How to ask

- Ask with `farik_ask_human`, one question per call, and end your turn after each, as
  `writing-task-contracts` says.
- Offer at most four choices when the answer is a choice, and leave room for the user's own words.
- Use plain words. No jargon, no names from the code, no acronyms the user did not use first.

## 4. When to stop

Stop asking once a new answer would not change the contract. Then write it. If you think you have
no question left, say so in the contract's intent; the user's approval is the check that you were
right.
