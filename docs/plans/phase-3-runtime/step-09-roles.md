# Phase 3, step 09: Role crate and the two launch roles

Status: ready
Branch: `phase/3-runtime`
Spec: `docs/SPEC.md` sections 5.1, 6, 6.1, 6.4; D7
Depends on: phase 1 and 2 (on main); nothing from this phase
Readiness confirmed by: fresh-session reviewer, 2026-09-22 (two rounds: the second on the one decision the first found open; findings folded in)

## Goal

Farik ships the Product Manager and the Software Developer as data: each role's mandate, what it produces, what it may not do, its default model and effort, its system prompt, and one skill, embedded in the binary and read through one loader. And Farik can say, for any contract and assignee, which role reviews the work (D7). Out of scope: roles other than these two (phase 4 ships the Scrum Master, the Architect, and the Marketing Specialist), a user's override of a role (phase 5, revision 8), and putting a role into a prompt (step 10).

## Decisions

- The crate is `farik-roles` in `crates/roles`, depending on `farik-core`, `serde`, `serde_json`, `serde-saphyr`, `jsonschema`, and `typify` (for `import_types!`, ADR 0009). No I/O at run time: the role files are embedded with `include_str!`, so the crate is as testable as `farik-core`. Every shipped role goes through one private `parse_role(role, yaml, system, skills: &[(&str, &str)]) -> Result<RoleDefinition, RoleError>`, which the tests call directly with broken copies.
- The role files live in `crates/roles/roles/<role_id>/` (revision 8): `role.yaml`, `system.md`, and `skills/<skill-name>/SKILL.md`, the layout 6 describes and code.md names.
- `role.yaml` is held to `docs/schemas/role.schema.json` (2020-12), validated with `jsonschema` before it is deserialised, as every schema-backed file is: `{ id, mandate, produces: [string], forbidden: [string], model: { id, effort }, skills: [skill-name] }`, `additionalProperties: false`, `id` one of the five agent roles (not `human`, which no agent is; a test holds the list equal to `Role`'s values less `Human`), `model.effort` `low | medium | high`, a skill name kebab-case. The generated types are the crate's own; `RoleDefinition` is the one hand-written type at its edge.
- A role's tiers are not in `role.yaml`: they are `farik_core::governor::permissions::default_tiers(role)`, the one place the governor reads them, and `RoleDefinition::default_tiers` is filled from there. Chose this over a second copy in YAML held equal by a test, because a copy the governor does not read is one a user will edit expecting an effect.
- Session limits are not in `role.yaml` either, for the tiers' reason: `budget_state` (step 03) reads `default_session_limits(role)` and the team's overrides, and a role file's copy nothing reads would be edited expecting an effect; phase 5's override adds the field together with its reader. `RoleDefinition` has no `session_limits` (revision 8 listed one).
- An agent's own `model` in the team file overrides the role's default model and effort; the orchestrator (step 11) applies it.
- A skill's `SKILL.md` opens with a `---` line, then YAML up to the next `---` line parsed with `serde-saphyr` into `{ name, description }` with unknown keys refused (the Agent Skills format; `name` equal to its directory), and `body` is the text after the closing line; a missing or malformed frontmatter is `RoleError::Invalid`. `Skill { name, description, body }` carries the text, and how a skill reaches a session is step 10's, which re-emits the frontmatter if it writes an Agent Skills folder. Revision 8's `Skill { name, dir }` is changed because an embedded skill has no directory.
- The two roles, from 6.1 and 6.4: the Product Manager on `claude-opus-5` at `high` effort, the Software Developer on `claude-opus-5` at `high` (8.2), a model the shipped price table prices; each `system.md` states the mandate, the forbidden list, that repository content, web pages, and tool results are untrusted (8.6), that `farik_exec` is the shell and git goes through the `farik_git_*` tools (ADR 0004), and how to end a session (ask the human, declare blocked, or request the transition). The skills are `writing-task-contracts` (Product Manager: triage, questions first for an epic, exit criteria from the library, the Definition of Ready) and `implementing-a-contract` (Developer: read the contract, work inside `allowed_paths`, commit, run every criterion and record it with evidence, write the completion note, request `verifying`).
- `load_role(Role)` answers `RoleError::NotFound` for the roles not shipped yet and for `Human`; `RoleError::Invalid` would mean a shipped file fails its schema, which a test over every shipped role rules out, so it exists for the phase 5 override. `EmbeddedRoles` and the `RoleSource` trait are cut (revision 8).
- `REVIEWER_ROLE_FOR: &[(Role, &[Role])]` is the preference for a task by its assignee's role, as D7 and 5.1 give it: Software Developer → Architect, then Software Developer; Architect → Product Manager; Marketing Specialist → Product Manager. The Product Manager's and the Scrum Master's own tasks have no row until phase 4 decides them, and get `None`. `default_reviewer_role(team, kind, assignee_role)` is what the contract's writer (the Product Manager, through `farik_write_contract`) fills `reviewer_role` with, before any assignee exists, so it takes the assignee's role, not an agent: the first preferred role the team can staff, counted as the Definition of Ready's `ReviewerAvailable` counts (two active agents when the reviewer's role is the assignee's, one otherwise), else `None`; a test holds that every `Some` it answers passes `ReviewerAvailable`. A Developer never reviews its own work because the assignment gate refuses a reviewer who is the assignee (D7). For an epic it answers `None`: an epic's reviewer is decided by the assignment gate (`gates.rs`: the Product Manager when the team has an active Scrum Master, else the human), which stays the one definition. The project plan's D7 line that "the assigning agent's prompt uses them" is corrected to the contract's writer.

## File map

```
Cargo.toml                                            modifies: farik-roles in the workspace
docs/schemas/role.schema.json                         creates
crates/roles/Cargo.toml                               creates
crates/roles/src/lib.rs                               creates: RoleDefinition, Skill, RoleError, load_role; tests
crates/roles/src/generated/mod.rs                     creates: import_types! of role.schema.json
crates/roles/src/reviewer.rs                          creates: REVIEWER_ROLE_FOR, default_reviewer_role; tests
crates/roles/roles/product_manager/role.yaml, system.md, skills/writing-task-contracts/SKILL.md      creates
crates/roles/roles/software_developer/role.yaml, system.md, skills/implementing-a-contract/SKILL.md  creates
docs/plans/project-plan.md                            modifies: step 09's interface line; D7's line about who uses the table
docs/SPEC.md                                          modifies: section 6, what `role.yaml` holds (tiers and limits are the governor's defaults, overridden per agent in the team file)
```

## Interfaces

Consumes: `Role`, `Team`, `TaskKind`, `default_tiers`, `PermissionTier`, `Effort`, `evaluate_readiness` (tests) (`farik-core`).

Produces:

```rust
pub struct Skill { pub name: String, pub description: String, pub body: String }
pub struct RoleDefinition { pub id: Role, pub mandate: String, pub produces: Vec<String>, pub forbidden: Vec<String>, pub default_tiers: Vec<PermissionTier>, pub model: String, pub effort: Effort, pub system_prompt: String, pub skills: Vec<Skill> }
pub enum RoleError { NotFound { role_id: String }, Invalid { role_id: String, detail: String } }
pub fn load_role(role: Role) -> Result<RoleDefinition, RoleError>;
pub const REVIEWER_ROLE_FOR: &[(Role, &[Role])];
pub fn default_reviewer_role(team: &Team, kind: TaskKind, assignee_role: Role) -> Option<Role>;
```

## Tasks

### Task 1: the schema, the loader, and the two roles

- `loads_the_product_manager` — mandate, `model == "claude-opus-5"`, `effort == High`, `default_tiers == default_tiers(ProductManager)`, one skill named `writing-task-contracts` with a non-empty description and body, and a system prompt that contains the word `untrusted`.
- `loads_the_software_developer` — the same, with its skill, and a system prompt that names `farik_exec`.
- `holds_every_shipped_role_to_its_schema` — each shipped `role.yaml` validates against `role.schema.json`, its `id` matches its directory, and every skill named has a directory whose frontmatter `name` equals it.
- `refuses_a_role_that_is_not_shipped` — `ScrumMaster`, `Architect`, `MarketingSpecialist`, `Human`: each `NotFound` naming it.
- `refuses_a_role_file_that_breaks_the_schema` — `parse_role` on a copy with an unknown key is `Invalid` naming the key.
- `refuses_a_skill_without_frontmatter` — `parse_role` with a `SKILL.md` that does not open with `---` is `Invalid` naming the skill.
- `lists_the_agent_roles_in_the_schema` — the schema's `id` enum equals `Role`'s values less `human`.

- [x] `feat(roles): ship the product manager and the software developer as data`

### Task 2: who reviews

- `prefers_an_architect_for_a_developers_task` — a team with an active Architect and two Developers: `Some(Architect)`.
- `falls_back_to_another_developer` — two Developers, no Architect: `Some(SoftwareDeveloper)`.
- `finds_no_reviewer_for_a_lone_developer` — one Developer: `None`.
- `sends_an_architects_task_to_the_product_manager` and `sends_a_marketing_specialists_task_to_the_product_manager` — each `Some(ProductManager)` with an active one.
- `answers_only_what_readiness_accepts` — for each assignee role and each of the fixture teams above, every `Some(r)` makes `evaluate_readiness`'s `ReviewerAvailable` pass for a contract with that `assignee_role` and `reviewer_role: r`.
- `passes_over_a_paused_architect` — an Architect paused, two Developers: `Some(SoftwareDeveloper)`; one active Developer and one paused: `None`, as readiness counts.
- `leaves_an_epics_reviewer_to_the_assignment_gate` — `kind: Epic`: `None` whatever the assignee's role.

- [x] `feat(roles): resolve which role reviews a task`

## Verification

```
cargo xtask check
# expected: xtask check: ok
```
