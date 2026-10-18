# Standalone: a team picked for the folder, models in plain sight, one agent alone

Status: ready
Branch: `feat/team-picker` (its own pull request to `main`)
Design: `docs/design/team-picker.md`; ADR 0071
Spec: `docs/SPEC.md` 4.1, 4.4, 5.2, 5.4, F1 (revision 0.125)
Depends on: `origin/main` at `d69ee53c`

## Goal

The founder's brief of 2026-10-18 (design, "The brief"). When done: setup's team step shows cards with the folder's team first; each agent's sheet sets its model by name, one of five efforts, and shows the temperature as set by the model; "One agent" starts a solo project whose requests run to a pull request with no team.

## Wire (fixed before any task)

- `team.propose` answer gains `suggested_teams: [{ "slug": string, "signal": "code" | "money" | "shop" | "new", "language": string | null, "tests": boolean }]`, every built-in slug once, best first. TypeScript: `suggestedTeams: { slug, signal, language, tests }[]`.
- `team.propose` answer also gains `role_defaults: { "<role wire name>": { "model", "effort", "persona" } }` for all eight roles, from `load_role`. TypeScript: `roleDefaults`.
- `models.list` → `{ models: [{ id, label }] }`, `label` "Opus 5.5" (family word + version digits joined by dots). `team.get`'s `effective[].model.label` the same; an older id "Opus 5 (older)".
- Effort enum everywhere (`team`, `team-template`, `role`, `event`, `rpc` schemas; Rust; TypeScript): `low | medium | high | xhigh | max`.
- `policy.solo?: boolean` in `team.yaml` (`docs/schemas/team.schema.json` `$defs/policy`, and the template policy), absent meaning false. Refusal codes `solo_one_agent`, `solo_role` at `/agents`.

## Tasks

- [x] **Task 1 (Rust).** Effort levels; model names; `suggested_teams` (`catervas_core::team::suggest_teams(tracked: &[String], language: Option<&str>, tests: bool) -> Vec<SuggestedTeam>`, the rules of design section 5); "Software product" gains Iris, the UI/UX Designer (`avatar: extra-1`, the Playwright connector as `suggested()` gives it). One test per functionality: suggestion order for code, money, shop and new folders; names from ids; five efforts accepted and passed to `--effort`.
- [x] **Task 2 (Rust).** Solo mode, design section 6, every hook the map names (`docs/design/team-picker.md` section 6). One test: a solo team of one Developer files a request through `request.file` and the task reaches `accepted` with no Product Manager session and no review session (the orchestrator harness's recorded sessions); plus validation's refusals.
- [x] **Task 3 (web).** Setup's team step as cards and the agent sheet; the Team page's agent editor's model, effort and temperature rows; the Team page in solo. Iterated against screenshots at 1280 and 390 px.
- [x] **Task 4.** `docs/SPEC.md` 0.125; `e2e/setup-team.spec.ts` and the web tests updated to the new screen.

## Verification

`cargo xtask check` (with `CARGO_BUILD_BUILD_DIR=$PWD/target`), its output pasted in the pull request; screenshots of the new step.
