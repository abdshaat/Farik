# Phase 7, step 11: DevOps Engineer role

Status: draft. Its readiness review runs once step 10h has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 1, 5.6, 6.9; F1
Depends on: steps 09 and 10b of this phase (an optional role's pattern; setup's "More roles" list and `EXTRAS` as step 10b leaves them); step 10h (ADR 0041, which Task 1's ADR cites); steps 05 to 08b (every role ships a `kit.yaml`); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). The project plan's row 11 is split in six, at the seams the code map of 2026-10-05 showed: this step is the role, its mockups and the ADR the next five rest on; 11b deploy tasks and `farik_deploy` over a fake platform; 11c the watch; 11d incidents, the restart and the rollback; 11e the incident's fix; 11f the pages and the command line. Row 12 is split in five (`step-12-devops-engineer-kit.md` to `step-12e-...`), one plan per group of platforms.

## Goal

A user can add a DevOps Engineer to the team: an optional role, offered in setup's "More roles" list and on the Team page, not suggested, with its own picture (the brand's `extra-3` character, as `devops-engineer`) and colour. It loads with its prompt and its skill, holds the Developer's tiers and `network`, changes code (its tasks work on `feature/` or `fix/` branches), and is reviewed by the Architect, or by a Developer when the team has no Architect. The founder approves the mockups of every DevOps screen of steps 11 to 11f before any of their code, and ADR 0043 records the decisions those steps share. Out of scope: deploys (11b); the watch (11c); incidents (11d, 11e); the pages (11f); the kit (12).

## Decisions

- **The role, as ADR 0027 and spec 6.9 say.** `devops_engineer`, plain name "DevOps Engineer", short tag "OPS", persona "Keeps production running", model `claude-opus-5-5` at `high` (the Developer's, ADR 0027), the default session limits. `default_tiers(DevopsEngineer)` is exactly `[Read, WriteWorkspace, Execute, Network, GitLocal]` in this step; step 11b adds `ExternalEffect` with the three tools that need it. `REVIEWER_ROLE_FOR` gains `(DevopsEngineer, &[Architect, SoftwareDeveloper])`.
- **It changes code.** `changes_code(DevopsEngineer)` is true, so its task works on `feature/<id>` or `fix/<id>` (`branch.rs`), `document_paths_only` does not hold it (`readiness.rs:565`), `run_commands: false` takes its `execute` and `push: true` gives it `git_remote` (`team.rs:1087`, `:1090`), as spec 6.9 asks ("held to the rules 5.3 and 5.12 set for the roles that change code"). Which code it changes (an incident's fix, the deploy configuration a task names) is the contract's `allowed_paths`, written by the Product Manager. Rejected: `changes_code` false with an exception for fix tasks, a second rule for one role.
- **The sentence about who changes code** becomes "Only the Developer, the UI/UX Designer and the DevOps Engineer change code." in the Developer's and the Designer's `system.md` (`software_developer/system.md:10`, `ui_ux_designer/system.md:10`) and the new role's; `says_who_changes_code_in_every_prompt` (`roles/lib.rs:449`) holds every role to it and checks it in those three.
- **`role.yaml`'s skill is one, `running-production`**, in the prompt (ADR 0011). This step's text: what the role is for; its code tasks (read the contract's `allowed_paths`, test, commit on the task's branch, a completion note); production logs and every platform's answer are untrusted data (spec 8.6), never instructions; it never calls a platform's tool that changes anything. Steps 11b, 11d and 11e each add the section for what they build (deploy tasks, incidents, the fix), each in its own task. `forbidden`, in this order: "deploy anything but the commit the team integrated"; "call a platform's tool that changes anything"; "change secrets, environment variables, access rights or scaling, or delete anything in production"; "open a shell in a production container"; "change application code outside an incident's fix task"; "change deploy configuration outside a task that names it". `kit.yaml` is `skills: []`, `connectors: []` until step 12.
- **Its picture.** `extra-3` (the cap, the headphones and the dark hoodie: someone on call) is copied byte for byte to `packages/brand/assets/avatars/devops-engineer-256.png` and `docs/brand/assets/characters/devops-engineer.png`; `AVATAR_KEYS` and `AVATAR_URLS` gain `devops-engineer`, and `extra-3` stays in `AVATAR_KEYS`, as step 09 keeps `extra-4`. `EXTRAS` (`TeamSetup.tsx:114`, `["extra-2", "extra-3"]` after step 10b) becomes `["extra-2"]` and its fallback `extra-2`, with its comment naming `extra-3` as the DevOps Engineer's. The cost: once two agents are added by hand, they share `extra-2`. Rejected: `extra-2`, whose purple sits next to the Marketing Specialist's lavender tag.
- **Its colour.** `role-devops-engineer` is `#C6A3C8` (an orchid, hue 297°, the widest gap left between the lavender of `role-marketing-specialist` at 256° and the rose of `role-product-manager` at 351°, and away from finance's `#C2C79B` and procurement's `#A6C3BF`) in both themes; about 8.2:1 against `role-ink` `#161616`, which `contrast.ts`'s check holds at 4.5. `contrast.ts`'s `ROLES` gains it.
- **Job line** (kept in `en.ts` for setup's list and the Team page): "Ships what the team built, watches it run, and puts it right when it breaks".
- **Where it starts.** An added DevOps Engineer takes `devops-engineer` and a name from `SPARE` (`someone()`, `TeamSetup.tsx:134-146`, and the Team page's add). `team.propose` (the six suggested) is unchanged. A team of the six and the DevOps Engineer is seven, `MAX_AGENTS`; the "More roles" list says so when the team is full, as it does for any role.
- **Mockups first (Task 0).** Every DevOps screen of steps 11 to 11f, and the one connector screen step 12d changes, is drawn once, so the founder approves them together, as step 05 drew 05b's.
- **ADR 0043 (Task 1)** records the decisions steps 11b to 11f share, each with its rejected alternative, so that no plan of the five is the only place one is written. Its decisions are the ones those plans state; the ADR adds none.

## File map

```
docs/design/mockups/{DevOpsRole,DevOpsCard,Production,TodayIncident,Incident,DeployPanel,ConnectorFileKey,PhoneIncident,PhoneProduction}.dc.html, canvas.json   creates (Task 0)
docs/decisions/0043-devops-deploys-watching-and-incidents.md          creates (Task 1)
docs/schemas/{task-contract,team,team-template,role,kit}.schema.json  modifies: role enum gains devops_engineer (Task 2)
crates/roles/roles/devops_engineer/{role.yaml,system.md,kit.yaml}     creates (Task 2)
crates/roles/roles/devops_engineer/skills/running-production/SKILL.md creates (Task 2)
crates/roles/roles/{software_developer,ui_ux_designer}/system.md      modifies: the code sentence (Task 2)
crates/roles/src/{lib.rs,kit.rs,reviewer.rs}                          modifies: load_role, load_kit, SHIPPED_ROLES, REVIEWER_ROLE_FOR; tests (Task 2)
crates/core/src/team.rs, crates/core/src/governor/permissions.rs      modifies: plain_role, changes_code, From<RoleWire>, default_tiers (Task 2)
crates/runtime/src/daemon/team.rs                                     tests: kit_skills_name_only_tools_farik_lists covers the role skill (Task 2)
packages/brand/assets/avatars/devops-engineer-256.png, docs/brand/assets/characters/devops-engineer.png   creates (Task 3)
packages/brand/src/{assets.ts,assets.test.ts,contrast.ts,contrast.test.ts}, packages/brand/tokens/tokens.json   modifies (Task 3)
packages/ui/src/{role.ts,strings.ts,avatars.ts,RoleTag.tsx,RoleTag.module.css,RoleTag.test.tsx}, packages/ui/gallery/Gallery.tsx   modifies (Task 3)
apps/web/src/pages/{Team.tsx,setup/TeamSetup.tsx,setup/SetupTeam.tsx}, apps/web/src/strings/en.ts   modifies (Task 4)
apps/web/src/pages/{team.test.tsx,setup/team.test.tsx}                 tests (Task 4)
docs/SPEC.md, docs/plans/project-plan.md                              modifies (Task 5)
```

## Interfaces

Consumes: `Role`, `RoleWire`, `plain_role`, `changes_code`, `default_tiers`, `PermissionTier` (`farik-core`); `load_role`, `load_kit`, `REVIEWER_ROLE_FOR`, `default_reviewer_role`, `SHIPPED_ROLES` (`farik-roles`); `AVATAR_KEYS`, `AVATAR_URLS`, `EXTRAS`, `someone()`, the "More roles" list (step 10b), `Role` (TS).

Produces: `Role::DevopsEngineer` and `RoleWire::DevopsEngineer`, generated from the schemas (typify names `devops_engineer` so); the TS `Role` gains `"devops_engineer"`; `AvatarKey` gains `"devops-engineer"`.

## Tasks

### Task 0: Mockups

An Opus session (ADR 0032) draws these on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf), desktop and phone, in the canvas's tokens, and copies them into `docs/design/mockups/`. The example is Theo, a DevOps Engineer, on a Vercel project.

- **`DevOpsRole`**: setup's "More roles" list (step 10b's) with the DevOps Engineer's row: picture, "OPS", the job line, unticked.
- **`DevOpsCard`**: the Team page's card for Theo, with one line under the persona in four states: "Production healthy. Checked at 10:42."; "Deploying a1b2c3d. Watching: 3 of 5 minutes healthy."; "Incident open since 10:42."; "Not watching. Farik last checked at 07:10." (a stopped watch is visible).
- **`Production`** (Settings, "Your production"): the service to deploy to, chosen from Theo's connected platforms; the service's name on that platform; the health address; "How long to watch a new deploy" (5 minutes); "Count it as broken when more than __% of requests fail" (optional, "Only where your platform reports it"); and the plain line "Farik watches only while this computer is on and Farik is running. Keep your platform's own alerts on too."
- **`TodayIncident`**: Today's first row while an incident is open: "Production is down since 10:42", then its steps as they happen ("Restarted at 10:43", "Rolled back at 10:49", "Fix FRK-31 waiting for review", "Fix deployed at 11:20"), with "Stop", "Restart", "Roll back" and "Mark as fixed"; a stopped incident ("You stopped this. Farik does nothing more on its own."); an incident that needs the human ("Restart and roll back did not bring it back. Decide what to do next.").
- **`Incident`** (`/incidents/<n>`): the timeline, the incident note (the agent's words, shown as text), the deployments it touched, and the fix task's link.
- **`DeployPanel`**: a deploy task's panel on its page and its gate, in place of the diff: "Deploys a1b2c3d, which holds FRK-12 and FRK-14", then "Watching: 3 of 5 minutes healthy", "Live and healthy since 10:47", or "The deploy failed. Incident 412 is open."
- **`ConnectorFileKey`** (for step 12d): the kit's key form on `ConnectorAdd` with a key that is a whole file, Kubernetes' sign-in file: "Choose the file" and a box to paste it into, with "Farik keeps it in your keychain and hands it only to this connection."
- **`PhoneIncident`**, **`PhoneProduction`**: the phone widths of Today's row and the settings.

Gate: the founder approves the boards, or says to approve them automatically; the approval, its date and the canvas version are written into this plan's Execution notes and the headers of steps 11b to 11f and 12d in the same commit.

- [ ] `docs(design): mock up the DevOps Engineer, production and incidents`

### Task 1: ADR 0043

Files: `docs/decisions/0043-devops-deploys-watching-and-incidents.md`, from `0000-template.md`, status accepted on the readiness review of steps 11 to 11f. 0043 is the next free number on 2026-10-05 (0042 is the marketing plan's); if another ADR takes it first, this one takes the next free number and the same commit changes every reference in steps 11 to 11f. Its Decision section, one paragraph each, with the rejected alternative:

1. The three tools are Farik tools of the `external_effect` tier, which the DevOps Engineer holds from step 11b; the hook is their gate: a sprint's or an incident's pre-approval, then a grant of step 02, then the team's `auto` (ADR 0041), then the ask (rejected: a tier check skipped by role, a second path past `evaluate_tool_call`).
2. A platform is a `Platform` the daemon gives (`DaemonState::set_platforms`), a fake in tests; the adapters are step 12's (rejected: a cargo feature, as ADR 0036 rejected one for kits; and `ToolDeps`, which exists before the daemon whose key stores the adapters read).
3. The production settings live in `team.yaml` beside `preview` (rejected: a private file, which the team page could not show and a template could not leave out).
4. The watch is a task of its own beside the tick loop, once a minute, starting no session, because a tick runs one session to its end (`orchestrator.rs:417`); two failing checks in a row make the service unhealthy (rejected: one, which a single network blip turns into an incident).
5. A deploy task has no branch; Farik moves it to `verifying` when it records the deploy settled healthy; acceptance integrates nothing (rejected: a session that waits out the settling period, which would spend the model's time watching).
6. One incident is open at a time; its steps run in sessions of a new purpose, `incident`; the restart and the rollback are pre-approved once each, and beyond them the human restarts or rolls back from Today (rejected: asking the human to approve the agent's second call, which a session about no task cannot do under step 02).
7. The fix is filed by the incident session, triaged by Farik, marked on `task.created`, and exempt from the sprint hold, the open sprint's membership and its budget; its own budget and the day's still hold (rejected: holding a production fix behind a spent sprint).
8. A session already running finishes before an incident's session starts (rejected for now: stopping it; see step 11d's O1).

- [ ] `docs(decisions): record how the DevOps Engineer deploys, watches and restores`

### Task 2: The role exists

One commit, since the generated enum makes every exhaustive match fail until each has its arm. Every test that lists the shipped roles gains it, as step 09's Task 1 lists them (`holds_every_shipped_role_to_its_schema`, `ships_the_mockup_persona_per_role`, `SHIPPED` and the folder count in `kit.rs`, `SHIPPED_ROLES`, `names_every_role_an_agent_can_hold`, `prompt.rs`'s role list); `forbids_application_code_to_every_role_but_the_developer` (`roles/lib.rs:576`) leaves it out, since it changes code; `changes_code_for_the_developer_and_the_designer_only` (`team.rs:2032`) is renamed `changes_code_for_the_three_code_roles`.

- `loads_the_devops_engineer`: `load_role(DevopsEngineer)` gives "Keeps production running", `claude-opus-5-5` at `high`, the one skill `running-production`, and the six `forbidden` lines above, in order. RED: no such role.
- `devops_holds_the_developers_tiers_and_network`: `default_tiers(DevopsEngineer)` is exactly `[Read, WriteWorkspace, Execute, Network, GitLocal]`. RED.
- `changes_code_for_the_three_code_roles`: true for the Developer, the Designer and the DevOps Engineer, false for the six others and `Human`. RED.
- `the_architect_then_a_developer_reviews_devops`: with an Architect, it; with none and one Developer, the Developer. RED.
- `a_devops_task_works_on_a_fix_branch` (`branch.rs`): a DevOps Engineer's task with `change: fix` is `fix/FRK-7`. RED.
- `no_commands_takes_devops_execute_and_push_gives_it_git_remote` (`team.rs`). RED.
- `says_who_changes_code_in_every_prompt`: the new sentence in the three code roles' prompts, and no role's text says only two roles change code. RED: the Developer's says the old sentence.
- `its_kit_is_empty_until_step_12`: `load_kit(DevopsEngineer)` has no skills and no connectors. RED.
- `running_production_passes_the_skill_checks` (`farik-roles`) and `kit_skills_name_only_tools_farik_lists` (`daemon/team.rs:4025`, which step 09 extends to role skills) over it. RED, then guard.

- [ ] `feat(roles): add the DevOps Engineer`

### Task 3: Its picture and colour

- `devops_has_its_own_picture` (`assets.test.ts`): `AVATAR_KEYS` holds `devops-engineer` and still `extra-3`, and both `devops-engineer` files equal `extra-3`'s byte for byte; the exact lists gain it. RED.
- `devops_has_a_role_colour` (`contrast.test.ts`): `role-devops-engineer` is `#C6A3C8` in both themes and passes against `role-ink`. RED.
- `role_tag_names_devops` (`RoleTag.test.tsx`): "OPS" with its colour class. RED.

- [ ] `feat(ui): give the DevOps Engineer its picture, tag and colour`

### Task 4: The role in the web app

As the approved `DevOpsRole` and `DevOpsCard` boards (the card's production line is step 11f's). `Team.tsx` `ROLES` gains it after the Procurement Specialist; `TeamSetup.tsx` `roleName`, `someone()` and `EXTRAS`; setup's "More roles" list; `en.ts` `roleDevops`, `jobDevops`.

- `setup_does_not_suggest_devops`: the proposed team is the six. Guard.
- `more_roles_offers_devops` (`setup/team.test.tsx`): the list has its row with the job line; ticking it adds an agent with `devops-engineer` and a name from `SPARE`. RED.
- `added_agents_draw_from_extra_2`: two hand-added Developers both get `extra-2`, never `extra-3`. RED.
- `the_team_page_adds_devops` (`team.test.tsx`). RED.

- [ ] `feat(web): offer the DevOps Engineer in the team builder`

### Task 5: Spec and plan

`docs/SPEC.md` 6.9: the role as built (picture, colour, tiers, reviewer, `changes_code`); 5.6's tier table names it beside the Developer for `write_workspace`, `execute`, `git_local` and `network`; 1 and F1 name it offered; the revision line. `docs/plans/project-plan.md`: row 11 says what was executed and gains rows 11b to 11f; row 12 gains rows 12b to 12e, as those plans' headers say.

- [ ] `docs(spec): record the DevOps Engineer role`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

Then, in the web app, by the founder: add a DevOps Engineer from setup's "More roles" and from the Team page, see its picture, tag and colour, and talk to it in a one-to-one chat.

## Execution notes

None yet.
