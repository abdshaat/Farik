# 0018. A shipped catalogue of skills and connectors, suggested by role

Date: 2026-09-26
Status: accepted

## Context

Spec F9 gives each agent its own skills and MCP servers (connectors). The phase 8 plan configured them by hand: a skill folder dropped into `.farik/agents/<id>/skills/`, and a connector written into `team.yaml` as a command or a URL. That suits a technical user. Farik's first user is not technical (spec 2).

On 2026-09-26 the founder decided three things:
- Each agent's page in the app lists recommended skills and connectors.
- Each role has its own suggestions, based on the agent's job.
- The user adds any of them to that one agent with a button.

The options for where the recommendations come from were these:
- **A catalogue shipped with Farik.** Each entry is written, checked and pre-labelled by the project, and each role names its suggestions from it. It works offline (spec 10), and every entry has been reviewed.
- **A live registry fetched from the internet.** It is always current. But it fails offline, it lists servers nobody has checked, and a registry that someone else controls would decide which tools an agent is offered.
- **No recommendations, only a search box.** This is the simplest to build, but a non-technical user does not know what to search for.

## Decision

Farik ships a catalogue of skills and connectors with each release. Each role file names the skills and connectors suggested for that role, from the catalogue. The agent's page shows those suggestions, each with an Add button that adds it to that one agent. The spec (section 6.6) lists the suggestions. The catalogue and the Add flow are built in phase 8, steps 01 and 02.

## Consequences

- A non-technical user gets a useful team without knowing what MCP is. Every connector they are offered has been checked by the project, and its tools come labelled with safe defaults.
- A catalogue entry never grants a permission. Tools that change things outside the computer still need the user's approval each time (spec 5.6). The role's limits still hold: a connector added to the Marketing Specialist does not let it change code.
- The catalogue is work to keep. Every connector must be checked before it ships: who publishes it, its license, how it signs in, and what each of its tools does. It must be checked again when the service changes it. The catalogue is updated only with releases, so a new connector waits for the next release. A user who cannot wait adds a custom server under Advanced.
- Skills in the catalogue are written or vetted by the project and ship under Apache 2.0, like the roles' own.
