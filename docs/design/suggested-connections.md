# Suggested connections

Status: built on 2026-10-11 on `feat/suggested-connectors`; drafted the same day in a Claude Code session from the founder's request ("Suggested connectors based on the user's project. For example, if the marketing agent needs Buffer to start posting on social media, the user will see a list of suggested connectors on the main page alongside who needs it"). The founder was not present for the brainstorm; the decisions marked **assumed** are the session's reading and stand until the founder rules otherwise.

## Why

A role's kit names the services that make the role better (`docs/design/role-kits.md`): Buffer lets the Marketing Specialist send the posts of an approved plan, Linear lets the Product Manager read the backlog, Stripe lets the Finance Specialist read the books. Every one is optional, and today they are offered in one place only: the agent's own page, under "From the Marketing Specialist's kit". A user who built a team and never opened an agent's page does not know those services exist, and finds out when the agent cannot do what it was hired for. The main page should say, before that happens, which services the team's agents could use and who needs each, with the way to connect it one click away.

## What the user sees

1. **A section on Today, "Suggested connections",** under "Waiting on you" and above the seller mail, renewals and the posts going out. It appears only while there is something to suggest; with nothing to suggest there is no section, not an empty one.
2. **One row per service and agent.** Each row shows the service's logo and title, then the agent's name and the kit's one-line "about" ("Mira · Schedules social posts"), with the kit's "why" behind the usual info button. Two actions: **Connect**, which opens that agent's page with the service's connect dialog already open, and **Not now**, which removes the row for good.
3. **At most four rows show at first.** With more, a line under them says "Show all 9" and opens the rest; "Show fewer" folds them back.
4. **On a phone** the section is the same, under the same heading, the rows stacked as Today's other rows are.

## What is suggested

For each **active** agent of the team, each service its role's kit offers that the agent does not have yet: no entry of that name in the agent's `mcp_servers`. Left out:

- a service already connected, or connected and needing "Connect again" or "Sign in again": the agent's page handles those, and Today's "Waiting on you" is not the place for a stale connection (**assumed**; a later change may add a Today row for a connection that stopped working);
- a service this build cannot connect yet (`at_launch`: Google Ads before Catervas Cloud, ADR 0044), since **Connect** would lead nowhere;
- the UI/UX Designer's built-in browser, which is not a service the user connects;
- a paused or retired agent's services (**assumed**: a paused agent is not working, so nothing it lacks is pressing; the suggestion returns when the agent is resumed);
- a service the user said **Not now** to, for that agent.

Rows are in the team file's order of agents, and within an agent in the kit's order of services, which is the kit author's order of usefulness. Nothing ranks them by the project's content (**assumed**: the request's "based on the user's project" is read as "based on the team the user built for this project"; reading the marketing plan to put Buffer first, say, is out of scope).

A service the user connected and later disconnected is suggested again, since the agent lacks it again; **Not now** silences it (**assumed**; the alternative, treating a disconnect as a Not now, hides a service the user may have disconnected only to connect another account).

## How it works

- **No new query.** `team.get` already answers `kits` (each role's connectable services, `at_launch` marked) and the team with each agent's status and `mcp_servers`; the page works the list out from them.
- **Not now is remembered on this computer**, in `.catervas/local/connector-suggestions.json` as `{ "hidden": [ { "agent_id", "server" } ] }`, beside `keys-copied.json` and shaped like it (ADR 0053). A new command, `connector_suggestion.hide { agent, server }`, adds a pair; `team.get` answers the file's pairs as `hidden_suggestions`. The command refuses, with `suggestion_not_offered`, a pair the team does not offer: an agent not in the team, or a service its role's kit does not list. It records no event: local UI state, as `keys_copied.dismiss` is. A file that cannot be read counts as empty.
- **Connect** is a link to `/team/<agent>?connect=<service>`. The agent page, as it does for `?allowances=<service>`, reads the parameter on arrival and opens the kit service's connect dialog when that service is in the role's kit and not held by the agent; otherwise the parameter is ignored and the page shows as usual.
- The section's text is externalized in `strings/en.ts` like every other string, and reuses Today's row styles.

## Out of scope

Ranking suggestions by the project's content (plans, tasks, the repository); suggesting a service to a paused agent; a Today row for a connection that needs "Connect again" or "Sign in again"; an "undo" for Not now (the agent's page still offers every kit service, so nothing is lost); suggestions for a user's own servers (a kit is the only source of a suggestion).

## Testing

Rust: `connector_suggestion.hide` writes the pair and `team.get` answers it; a second hide of the same pair writes it once; a pair the team does not offer is refused `suggestion_not_offered` and nothing is written; an unreadable file answers no pairs. Web: Today shows a row for an active agent's kit service it lacks, with the agent's name, the kit's about and why, and a Connect link to `/team/<agent>?connect=<service>`; it shows no row for a connected service, an `at_launch` service, a paused or retired agent, or a hidden pair; Not now calls the command and removes the row; four rows then "Show all N"; no section when nothing is suggested; no accessibility violations. The agent page opens the connect dialog for `?connect=<service>` and ignores a service not in the kit or already held.

Docs: `docs/SPEC.md` 6.7 and F9. No ADR: no decision of the spec is reversed, and the one new file follows ADR 0053's precedent.
