# 0007. Slack is the front door, the web app holds settings and records, Notion is a destination

Date: 2026-10-24
Status: proposed

## Context

The founder wants the agent to live "inside the workspace they use", naming Slack and Notion. Catervas has its own web app as the only surface. Building chat between people is a fight Praeco would lose (the earlier analysis: Slack, Teams, OpenAI, Notion and Linear all ship agents where people already work). Slack's developer policy forbids training a model on Slack data, and its API terms (mid-2025) forbid bulk export, long-term copies of Slack data, and using it in LLMs.

## Decision

- **Slack** is where people talk to Praeco: mentions and direct messages become requests (SPEC 4.3), approvals arrive with buttons (4.4), the digest is posted to one channel (4.2). Praeco reads only messages addressed to it (`app_mention`, `message.im`), never a channel's history, and keeps only the text of the message that asked, on the job it created. It never trains on any of it. Self-hosted installations use Socket Mode by default, so no public address is needed; the hosted service uses Slack's HTTP events.
- **The web app** is where settings and records live: setup, connections, members and roles, rules, budgets, the prospect inbox, SEO, documents, the playbook, the activity log.
- **Notion** is a destination: research documents and the weekly report are written as pages under one parent page the owner picks, and kept in sync. Praeco reads only the Notion pages an owner points it to as the product's documents.
- Microsoft Teams, Discord and Google Chat are not built before `v0.1.0`.

## Consequences

Praeco shows up where the people already are, and the web app stays small. Requests and approvals have one path each.

What cuts against it: Praeco depends on Slack's terms and its Marketplace review, which the SPEC records as open question 1, with the fallback that requests move to the web app; and a company on Teams cannot use Praeco until a later phase adds it.
