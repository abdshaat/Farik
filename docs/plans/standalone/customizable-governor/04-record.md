# Customizable governor, plan 4: the record

Status: ready
Branch: `feat/customizable-governor`, after plans 1 and 2, with plan 3 merged in

## Task 18: the spec says it

Files: `docs/SPEC.md`
Writes revision 0.135 (2026-10-20, ADR 0080) at the end of the revision paragraph, and changes, each in the section that holds the rule now:
- 5.5: an agent's own session limits, applied after the team's.
- 5.6: the shipped protected paths can each be lifted (`rules.lifted_paths`); read-only paths (`rules.read_only_paths`, refusal `path_read_only`); Catervas's own state is never written and `daemon.json` and `sessions/**` never read, whatever the rules; git in `catervas_exec` is the read-only eight plus `rules.git_subcommands`; the `ask` label, which asks like `paid` and costs nothing.
- 5.7: readiness escalates after `policy.readiness_attempts` failures (default 3).
- 5.12: the per-agent rules (`agents[].rules`) and how they combine with the team's; the owner sets every rule on Settings → Rules and the agent's page; agents still never change them.
- 5.14: `catervas_git_push` pushes to a remote in `policy.push_remotes` (default `origin`), refusal `remote_not_allowed`; Catervas's own integration is unchanged.
- 8.6: the session's deny rules (`Read` for off limits and Catervas's keys, `Edit` for read only and Catervas's state); "Secrets stay out of reach" now reads "unless the owner lifts them".
No test: the spec is prose (hard rule 8 is the check).

- [x] `docs(spec): the owner sets the governor's rules, revision 0.135 (ADR 0080)`
