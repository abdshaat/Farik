# 0055. Three ready-made teams ship with Catervas, and a team needs only a Product Manager

Date: 2026-10-10

Numbering: 0054 is claimed by the setup-friction branch (`feat/setup-friction`) and 0051 by the Catervas folders branch (`phase/8-catervas-folders`).
Status: accepted (the founder, 2026-10-10, approving `docs/design/predefined-teams.md` from a brainstorm in a Claude Code session: "A, pull the rule change forward now"; "A, one desk, three specialists"; "Without brief"; "Just predefined teams for these 3 purposes"; "make sure the user is able to create a custom team and add more roles to his team later on"; "Now implement the plan"). Amends ADR 0040 decision G5 in its timing, ADR 0026 part C in what a template can be, and ADR 0019 in the Finance Specialist's mandate.

## Context

Setup builds a team from the suggested six, a saved team, or from scratch, and every team needs an active Product Manager and an active Software Developer (D18; spec 1 and F1). The founder asked for ready-made teams for three purposes, picked at setup: a software product team (Product Manager, Architect, Software Developer, Marketing Specialist, and the DevOps Engineer once it exists), an online store team (Product Manager, Procurement Specialist, Finance Specialist, Marketing Specialist), and a financial analysis team (a Product Manager and three Finance Specialists for portfolio management, risk analysis and opportunity analysis). Two of the three have no Software Developer.

ADR 0040 (G5) decided that a team can run without a Software Developer but planned the change for Business workspaces, after the web launch, together with workspaces that are not repositories. The founder's consulting plans of 2026-10-10 make business teams the first thing clients need, ahead of the launch.

The Finance Specialist's folder runs one task at a time whoever holds the role (`docs/design/finance-specialist.md`), and its mandate speaks of the product's costs and earnings, management accounting and never financial advice.

## Decision

1. **A team needs an active Product Manager, two to seven agents, and nothing else.** D18 becomes "at least one Product Manager". The team editor, setup's check and a template's check refuse a team without one and no longer one without a Software Developer. The Definition of Ready gains a rule: the team has at least one active agent of the contract's assignee role, refused in words that name the role to add. This replaces G5's `needs_a_builder` refusal by contract kind and criteria: the assignee rule covers a Developer's task in a store team and a finance task in a software team alike, and the reviewer rule (5.3) already stops a Developer's task when neither an Architect nor a second Developer is active. The rest of G5 is unchanged and stays with Business workspaces: a task that changes code still needs a repository, and every project is still one.
2. **Three ready-made teams ship with Catervas as built-in templates**, in the saved-team format, embedded in the binary and offered as a fourth start at setup and above the saved teams on the Team page, with the same preview and the same checks. They cannot be renamed or deleted, and a saved team cannot take their names. They are there without a state folder. No starting brief: the user files the first request.
3. **The financial analysis team is one desk with three specialists.** The three Finance Specialists share the books folder and work one task at a time; each has a persona for its desk, portfolio, risk or opportunities, and the Product Manager gives each task to the one whose desk it is and reviews it. A folder per analyst was rejected: it changes the finance folder, its history and baselines, every finance tool, the daily sweep and the review path, and would be a step of its own.
4. **The Finance Specialist's mandate is widened** from the product's costs and earnings to a business's money, what it owns, spends, earns and has at stake, so portfolio, risk and opportunity analysis is its work. Its guards stay: management accounting, not financial advice, said wherever it gives numbers; every number names its source; it never pays, refunds or moves money. The business-neutral rewrite of every role's prompt stays with Business workspaces.
5. **Custom teams stay.** "From scratch" starts with the Product Manager's row, fixed, and the Developer's, which can be unticked; "Add someone" and "More roles" add the rest; the Team page's "Add someone" offers every role after setup, as today.

## Consequences

- Phase 14 (Business workspaces) step 03, "A team without a Developer", is done early in its team-rule part; its builder question ("does the business build software?") and the repository-less workspace stay there. Step 05, "Business templates", starts from these three; the founder's car flipping, Amazon resale and car rental teams, each with a brief, are still that step's.
- A project's starter team (`catervas init`) stays a Product Manager and a Software Developer: it is the software default, and a software project is still what `init` makes.
- `needs_developer` is gone from the daemon's refusal codes and the pages' words; `template_builtin` is new.
- Spec revision 0.103; the project plan's D18 line and revision 43.
