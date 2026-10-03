---
name: prioritising-the-backlog
description: Use when deciding what comes next on the backlog, so that the order has reasons.
---

# Prioritising the backlog

The order of the backlog is a decision. Make it from evidence, and write the reason down.

## 1. Value against effort

For each candidate, state the value (who gains and how much) and the effort (the task's budget and
risk). Name the evidence for the value: a usage number, a customer's words, a deadline. If there is
none, say so.

## 2. The cost of waiting

Ask what gets worse if it waits a sprint: a deadline passes, users keep hitting a fault, other
work stays blocked. Work with a real cost of waiting moves up.

## 3. No ties

Give every item its own place. Each order gets its reason in one line, for example "1. Fixes the
sign-in fault users report most; two tasks wait on it." A tie hides a decision you have not made.

## 4. Mark a guess as a guess

Where the value or the effort is your estimate, write "guess". Do not dress it as a measurement.
Ask the user (`farik_ask_human`) when a guess decides the order and they would know.

## 5. Record it

Farik keeps no priority field. When the session lets you post, put the order and each reason in the
team channel with `farik_post_message`, without naming anyone; the sprint's planning reads the
channel. Say only what changed since your last order.
