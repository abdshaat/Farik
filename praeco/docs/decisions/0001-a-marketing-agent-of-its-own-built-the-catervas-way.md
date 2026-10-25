# 0001. A marketing agent of its own, built the Catervas way

Date: 2026-10-24
Status: proposed

## Context

Catervas runs a governed team of up to seven agents on one person's computer. Its Marketing Specialist (Catervas ADRs 0042, 0072) is its most complete role: a research foundation, 23 skills, Buffer, Google Ads with a hard budget stop, Higgsfield and Recraft. The founder decided to bet on that one agent as a product: "a marketing agent that lives inside the workspace they use", "easily configurable and learns by itself", "long running and working constantly either on scraping the web about the product or the problem it's trying to solve, collect potential customers, and optimize SEO", in a public repository with a fully hosted option.

The options were: fork the Catervas repository and cut it down; build the new product inside Catervas as a mode; or start a new repository that carries over the designs, the skills and the rules, but not the code. Catervas's code assumes one human, one computer, a git repository as the unit of work, Claude Code processes supervised locally and a Docker sandbox; about 184,000 of its 317,000 lines of Rust do that, and none of it fits an agent that runs while no laptop is on and answers in a company's Slack.

## Decision

Praeco is a new public repository. It carries over from Catervas, by copy and with a link to the source: the Marketing Specialist's skills (`crates/roles/roles/marketing_specialist/skills/`), the research foundation's design, the rules for plans, posts and budgets, the sources policy, and the harness's principles (a contract before work, a governor in code, bounded everything, an append-only log). It does not carry Catervas's code.

It is built the way Catervas is: the same six-stage workflow (`docs/standards/workflow.md`), two-level planning with step plans from `docs/plans/step-template.md`, test-driven execution with one test per major functionality, Opus 5.5 planning and Sonnet 5.5 executing, no agent review of plans or code, and one phase per branch and pull request, reviewed and merged by the founder.

## Consequences

The new repository starts small and fits its product: hosted, multi-person, Slack-first. Catervas stays as it is, public and frozen at its current state; nothing in this plan changes it.

What cuts against it: nothing of Catervas's 3,800 tests comes along, and the governor, the event log and the budgets are written again, in TypeScript. The designs and rules are settled, which is most of the work; the code is the smaller part, and Catervas's crates remain readable as a reference for every rule ported.
