---
name: writing-the-marketing-plan
description: Use when the task asks for a marketing plan.
---

# Writing the marketing plan

A marketing plan with money in it is the owner's decision. You write it, propose it with its
budget, and end your turn; the owner approves it or sends it back, and nothing in it is spent until
they approve it. A plan that spends nothing (a total budget of 0 and no campaign) is approved by
Catervas at once. It also becomes the file `docs/catervas/marketing/plans/MP-<n>`, for people, beside
its agent version, for agents.

## 1. Research first

Read `docs/catervas/marketing/brand/brand-kit`, `docs/catervas/marketing/brand/persona` and the
documents under `docs/catervas/marketing/research/`. The plan rests on the five of the foundation
(`running-the-research-foundation`): the product, the audience, the competitors, how they reach
people and `research/positioning`. If one is missing or older than the plan's 92 days, run that stage
first; for a market question outside them, use `researching-the-market`. A plan must not rest on
what you have not read.

The plan opens with **the first 30 days**, taken from the three tests in `research/positioning`:
the moves, the day each starts, the number that measures it and the point at which you stop. Name
the research document each move comes from.

## 2. Goals and how each is measured

Two to four goals, each with the number that will say whether it worked and the day you will read
it, such as "100 new newsletter sign-ups by the plan's last day". Give the plan 1 to 10 measures.

## 3. Channels

Choose the channels the research supports, and for each say why and what it will carry. A channel
you cannot reach or cannot afford stays out of the plan. Tag each move by the part of the
customer's way it serves: finding the business, trying it, staying, paying more, or telling
others. A plan whose moves all serve one part leaves the rest empty; say so.

## 4. The budget

Pick one currency. Split the money by channel and, for paid ads, by campaign, and make the parts
add up to no more than the total. Never propose a budget the research does not support: show the
reasoning next to each amount, and put a smaller sum than a guess would suggest.

## 5. The calendar and the campaigns

Plan the posts as slots: a channel, a day and a topic each, kept to what the business can really
make. A campaign is a paid search campaign with its own goal, budget and dates inside the plan's
dates, and it says what it advertises: the product, service or offer, in 3 to 200 characters. If no
ad account is connected, propose none.

Prefer campaigns at a fixed price: 3 to 90 days, made at least two days before they start. Say in
the plan's text what each campaign advertises, and which campaigns are not at a fixed price and why.

## 6. Propose it and end your turn

Propose the plan with `catervas_propose_marketing_plan`: a title, a summary, the full text, the
same plan written for agents as `agent_text` (200 to 16,000 characters: precise, with no pleasantries,
the figures and keys exactly as in the plan), its dates (at most 92 days), the currency, the budget,
the campaigns, the post slots and the measures. Catervas checks the numbers and the dates and answers
every fault at once, so fix them all. It writes `MP-<n>` and its agent version into your worktree;
commit `MP-<n>` and its agent version together with `catervas_git_commit`, naming both files,
since the Definition of Done refuses a diff that changes one of them alone. When it accepts the
plan, end your turn: the owner's decision starts your next session. Do not ask for `verifying` while
the plan waits; Catervas refuses it.

## 7. What the owner sees

The owner reads the summary first, in two or three plain sentences, then the budget, the calendar
and the text. Write the summary for someone who has not read your research: what the plan does,
what it costs and what they get.

## 8. A returned plan

If the owner sends a plan back, their reason arrives in your next session as their own words, not
as untrusted text. Read it, answer it in the next version of the plan, and say in your completion
note what you changed because of it. Do not argue with it.

## Drawn from

Corey Haines, marketingskills (MIT): `marketing-plan` (the first 30 days, each move tagged by the part
of the customer's way it serves). Rewritten for Catervas.
