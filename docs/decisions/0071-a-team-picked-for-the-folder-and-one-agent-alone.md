# 0071. A team picked for the folder, each agent's model in plain sight, and one agent alone

Date: 2026-10-18
Status: accepted (the founder, 2026-10-18, quoted in `docs/design/team-picker.md`; the mechanism decided by the planner the same day, the founder answering no questions on it)

Amends:
- **ADR 0055** (ready-made teams): setup no longer offers "the suggested team" as its own start; "Software product" gains the UI/UX Designer and is the team for a folder of code.
- **ADR 0063** (chat tasks): in a solo project every request is a given task, and a given task runs to its pull request with no Product Manager and no reviewer.
- **SPEC 4.1, 4.4, 5.2, 5.4, F1** (revision 0.125).

## Context

The founder found choosing a team hard (four radio starts, nested radio lists, a paragraph per role) and asked for teams suggested from the folder, shown as cards; for each agent's exact model, effort and temperature, shown and changeable; and for "a single customizable agent", which "may operate outside any team function". The runtime assumes a Product Manager in five places on a given task's way to its pull request (draft to refining, the product plan's wait, readiness's reviewer, assignment, acceptance), and the schema asks for two agents.

## Decision

1. **Suggested from the folder.** `team.propose` answers `suggested_teams`, every ready-made team in the order the folder's tracked paths fit them, each with the signal that put it there (`code`, `money`, `shop` or `new`), the scan's language and whether it found tests; a pure function in `catervas-core` orders them. The web shows them as cards, the first full width with "Suggested", then "One agent", saved teams and "Build your own".
2. **Models by their names.** A model is named as Anthropic names it ("Opus 5.5", "Sonnet 5.5"), from its id, in `models.list`, in the Team page and in the sentences that describe a change; "Strongest model, thinks hard" and its siblings go.
3. **Five efforts.** An agent's effort is one of `low`, `medium`, `high`, `xhigh`, `max`, the levels Claude Code's `--effort` takes; shown as "Low", "Medium", "High", "Extra high", "Max".
4. **No temperature control.** Agents run in Claude Code, which takes no temperature, and every model Catervas offers refuses one other than its default. The agent's sheet shows "Set by the model" with the reason behind an ⓘ. Rejected: a stored temperature nothing reads.
5. **One agent alone** is `policy.solo: true` on a team of exactly one agent of any role but the Scrum Master and the UI/UX Designer. In solo:
   - `request.file` files the owner's words as a task given to the agent (ADR 0063's path);
   - the given agent moves it to refining, the product plan's wait does not apply, readiness skips the reviewer rules, Catervas assigns it with no assigner and no reviewer, and Catervas verifies its criteria alone and accepts it;
   - its pull request is the owner's review; Catervas never merges;
   - epics, deliveries, the Catervafication and the product-plan interview do not run;
   - retiring the agent is refused; pausing it is allowed.
   Rejected: inferring solo from a team of one (a Product Manager alone is a team); a new "general" role (a role is a role file, a kit and a persona).

## Consequences

- A shop's folder or a folder of spreadsheets is offered a team for it first.
- An owner sees "Opus 5.5" and "High effort" on every agent, before and after setup.
- One agent's code reaches the owner unreviewed by any agent; the pull request and the contract's checks are what stand between it and the main branch.
- Turning one agent into a team is a new setup or "Use a saved team", not an edit in place.
