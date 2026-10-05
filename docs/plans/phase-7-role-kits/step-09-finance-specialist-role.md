# Phase 7, step 09: Finance Specialist role

Status: ready
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 1, 5.1, 6.6; F1
Depends on: steps 05 to 08b of this phase (the kit format, every role shipping a `kit.yaml`; step 08 landed as aef7384 to 824a4ea and its fixes, step 08b executed first, since both change `kit.rs`'s `loads_every_shipped_kit`); phase 6 (merged in #19)
Readiness confirmed by: a fresh Opus session, 2026-10-05 (one round, against `docs/standards/workflow.md` stage 2), with steps 09b and 09c; folded below

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). The project plan's row 09 is split in three, at the seams the code map of 2026-10-05 showed: this step is the role, offered and loadable; step 09b is its spreadsheet tools; step 09c is a finance task's life in its private folder (the working directory, readiness, `verifying`, the review's baseline, nothing to integrate, one piece of work at a time). Stripe is connected in step 10's kit, not by hand here, since kits exist now (the row's "Stripe connected through step 01's connector screen" predates step 05).

## Goal

A user can add a Finance Specialist to the team: an optional role, offered by the Team page's "Add someone", not suggested, with its own picture (the brand's `extra-4` character, as `finance-specialist`) and colour. Its role loads with its prompt and its skill, its tiers are `read` and `network`, and its reviewer is the Product Manager. Until step 09c a task assigned to it cannot finish (it holds neither `write_workspace` nor `git_local`, and `verifying` needs a commit, `gates.rs:448`): this step makes it addable and chattable, and the user can talk to it in a one-to-one chat. Out of scope: the sheet tools (09b); the finance folder and its task rules (09c); Stripe (10); setup's choice of optional roles, the "More roles" list mocked up in step 10b's Task 0, which lists this role and the Procurement Specialist.

## Decisions

- **The role, as ADR 0019 and spec 6.6 say.** `finance_specialist`, plain name "Finance Specialist", short tag "FIN", persona "Keeps your numbers straight", model `claude-sonnet-5-5` at `medium` (the Marketing Specialist's; the design's `claude-sonnet-5` predates 5.5), tiers exactly `[Read, Network]`, reviewer the Product Manager (`REVIEWER_ROLE_FOR` gains `(FinanceSpecialist, &[ProductManager])`), `changes_code` false, the default session limits. Rejected: the Architect as reviewer, who does not own the business's numbers.
- **`role.yaml`'s skill is one, `keeping-the-books`**, in the prompt (ADR 0011): what the role keeps (the team's AI spending first; then the product's costs and revenue), that its numbers are management accounting, not a tax filing, statutory accounts or financial advice, and it says so; every number with its source; never pay, refund, move money, change budgets or write to a service; and no line about where its work goes until step 09c, whose Task 7 adds "work in your private folder; ask for `verifying` naming your `workbooks`". `forbidden` is spec 6.6's "Cannot" in these six lines, in this order: "pay, refund, or move money"; "change Farik's budgets or anything in Stripe or a mailbox"; "send, delete, move, or mark any email"; "publish anywhere"; "write application code"; "write anything outside your finance folder". Its `kit.yaml` is `skills: []`, `connectors: []` until step 10.
- **Its picture.** The brand's `extra-4` character (ADR 0019) is copied to `finance-specialist`: `packages/brand/assets/avatars/finance-specialist-256.png` from `extra-4-256.png` and `docs/brand/assets/characters/finance-specialist.png` from `extra-4.png`, byte for byte; `AVATAR_KEYS` and `AVATAR_URLS` gain `finance-specialist`, and `extra-4` stays among the extras. Its colour, `role-finance-specialist`, is `#C2C79B` (a pale olive beside the six role colours, `tokens.json:19-24`, and procurement's planned `#A6C3BF`) in both themes, passing `contrast.ts`'s check against `role-ink` (about 10.3:1 on `#161616`).
- **No mockups, so only the Team page.** The Team page's "Add someone" is a select of roles (`Team.tsx:20-27`, `:324`), so a new option needs no mockup (`web-ui.md:164`). Setup's "Add someone" copies the Developer (`SetupTeam.tsx:87-108`) and has no role choice, so offering optional roles in setup is a new control: step 10b's Task 0 mocks up its "More roles" list, with this role in it, before any code. Rejected: a mockup task here, which would hold this step on the founder for a list 10b draws anyway. Job line (kept in `en.ts` for 10b's list and the Team page): "Keeps the books and forecasts your spending, starting with the team's AI costs".
- **Where it starts.** An added Finance Specialist takes `finance-specialist` as its picture and a name from `SPARE`: `someone()` (`TeamSetup.tsx:134-146`) and the Team page's add give a `finance_specialist` the avatar `finance-specialist`, and every other role keeps `EXTRAS`. `team.propose` (the six suggested) is unchanged.

## File map

```
docs/schemas/{task-contract,team,team-template,role,kit}.schema.json   modifies: role enum gains finance_specialist (Task 1)
crates/roles/roles/finance_specialist/{role.yaml,system.md,kit.yaml}  creates (Task 1)
crates/roles/roles/finance_specialist/skills/keeping-the-books/SKILL.md   creates (Task 1)
crates/roles/src/{lib.rs,kit.rs,skill_check.rs,reviewer.rs}  modifies: load_role, load_kit, SHIPPED, SHIPPED_ROLES, REVIEWER_ROLE_FOR; tests (Task 1)
crates/core/src/team.rs                                      modifies: plain_role, From<RoleWire>, names_every_role_an_agent_can_hold (Task 1)
crates/core/src/governor/permissions.rs                      modifies: default_tiers (Task 1)
crates/runtime/src/prompt.rs                                 tests: the role list (Task 1)
crates/runtime/src/daemon/team.rs                            tests: kit_skills_name_only_tools_farik_lists also checks role skills (Task 1)
packages/brand/assets/avatars/finance-specialist-256.png, docs/brand/assets/characters/finance-specialist.png   creates (Task 2)
packages/brand/src/{assets.ts,assets.test.ts,contrast.ts,contrast.test.ts}, packages/brand/tokens/tokens.json   modifies (Task 2): assets.test.ts:17-29 and :39 (10 avatars become 11), contrast.test.ts:63-71 and :125-130, contrast.ts:27 (ROLES)
packages/ui/src/{role.ts,strings.ts,avatars.ts,RoleTag.tsx,RoleTag.module.css,RoleTag.test.tsx}, packages/ui/gallery/Gallery.tsx   modifies (Task 2)
apps/web/src/pages/{Team.tsx,setup/TeamSetup.tsx,setup/SetupTeam.tsx}, apps/web/src/strings/en.ts   modifies (Task 3): ROLES, roleName, someone(), JOBS
apps/web/src/pages/{team.test.tsx,setup/team.test.tsx}         tests (Task 3)
docs/SPEC.md, docs/plans/project-plan.md                       modifies (Task 4)
```

## Interfaces

Consumes: `Role`, `RoleWire`, `plain_role`, `changes_code`, `default_tiers`, `PermissionTier` (`farik-core`); `load_role`, `load_kit`, `REVIEWER_ROLE_FOR`, `default_reviewer_role`, `SHIPPED_ROLES` (`farik-roles`); `AVATAR_KEYS`, `AVATAR_URLS`, `Role` (TS).

Produces: `Role::FinanceSpecialist` and `RoleWire::FinanceSpecialist`, generated from the schemas; the TS `Role` gains `"finance_specialist"`; `AvatarKey` gains `"finance-specialist"`.

## Tasks

### Task 1: The role exists

One commit, since the generated enum makes every exhaustive match (`default_tiers`, `plain_role`, `From<RoleWire>`, `load_role`, `load_kit`) fail to compile until each has its arm. Every test that lists the shipped roles gains it: `holds_every_shipped_role_to_its_schema` (`lib.rs`, its directory list), `says_who_changes_code_in_every_prompt`, `ships_the_mockup_persona_per_role`, `SHIPPED` and the folder count in `kit.rs`, `SHIPPED_ROLES`, `names_every_role_an_agent_can_hold` (`team.rs`), `copies_the_team_schemas_definitions` (holds by construction), `prompt.rs`'s role list, and `forbids_application_code_to_every_role_but_the_developer` (`roles/lib.rs:576`).

- `loads_the_finance_specialist`: `load_role(FinanceSpecialist)` gives the persona "Keeps your numbers straight", `claude-sonnet-5-5` at `medium`, the one skill `keeping-the-books`, and the six `forbidden` lines above, in order. RED: no such role.
- `finance_reads_and_researches_only`: `default_tiers(FinanceSpecialist)` is exactly `[Read, Network]`. RED.
- `the_product_manager_reviews_finance`: with a Product Manager and an Architect on the team, `default_reviewer_role` gives the Product Manager. RED.
- `finance_does_not_change_code`: `changes_code(FinanceSpecialist)` is false. RED.
- `plain_role_names_finance`: "Finance Specialist". RED.
- `its_kit_is_empty_until_step_10`: `load_kit(FinanceSpecialist)` has no skills and no connectors. RED.
- `keeping_the_books_passes_the_skill_checks` (`farik-roles`): the shipped skill passes `check_skill`. RED.
- `kit_skills_name_only_tools_farik_lists` (`daemon/team.rs:4026`, since `farik-roles` cannot reach `tool_descriptors`): it iterates `load_role(role).skills` as well as each kit's, for every shipped role, so `keeping-the-books` names only `farik_*` tools Farik lists (none before 09b). RED: a role skill naming `farik_nope` passes today; the executor shows it with a scratch line, then removes it.

- [x] `feat(roles): add the Finance Specialist`

### Task 2: Its picture and colour

- `finance_has_its_own_picture` (`assets.test.ts`): `AVATAR_KEYS` holds `finance-specialist` and still `extra-4`, and the two `finance-specialist` files equal `extra-4`'s byte for byte. RED.
- `finance_has_a_role_colour` (`contrast.test.ts`): `role-finance-specialist` is `#C2C79B` in both themes and meets the contrast the other role colours meet against `role-ink`. RED.
- `role_tag_names_finance` (`RoleTag.test.tsx`): "FIN" with its colour class. RED.
- The exact lists in `assets.test.ts` (`:17-29`, the length 11 at `:39`) and `contrast.test.ts` (`:63-71`, `:125-130`) gain it. RED with the rest.

- [ ] `feat(ui): give the Finance Specialist its picture, tag and colour`

### Task 3: The role in the web app

`Team.tsx` `ROLES` gains it after `marketing_specialist`; `TeamSetup.tsx` `roleName` and `someone()` (`ringOf` needs no change); `SetupTeam.tsx` `JOBS`; `en.ts` `roleFinance`, `jobFinance`.

- `setup_does_not_suggest_finance` (`setup/team.test.tsx`): the proposed team is the six, and none is a Finance Specialist. Guard.
- `someone_gives_finance_its_picture` (`setup/team.test.tsx`, through `someone()`): a `finance_specialist` gets `finance-specialist` and a name from `SPARE`; a Developer still gets an `EXTRAS` picture. RED.
- `the_team_page_adds_finance` (`team.test.tsx`): "Add someone" lists it with its job line, and adding it gives it `finance-specialist`. RED.

- [ ] `feat(web): offer the Finance Specialist in the team builder`

### Task 4: Spec and plan

`docs/SPEC.md` 6.6: the role as built in this step (picture, colour, reviewer; offered on the Team page, and in setup once step 10b's list lands; a task of its cannot finish until 09c); 1 and F1 name it offered; the revision line. `docs/plans/project-plan.md`: row 09 says what was executed, and gains rows 09b and 09c as this plan's header describes (09b: `farik_read_costs`, `farik_read_sheet`, `farik_write_sheet`, `rust_xlsxwriter` `=0.99.1` and `calamine` `=0.36.1`, the folder keyed by role; 09c: the private folder's rules); row 10 says Stripe is connected there, not in 09.

- [ ] `docs(spec): record the Finance Specialist role`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
```

Then, in the web app, by the founder: add a Finance Specialist from the Team page, see its picture and colour, and talk to it in a one-to-one chat.

## Execution notes

- Task 1: `the_finance_specialist_says_its_numbers_are_not_a_filing_or_advice` (`crates/roles/src/lib.rs`) is beyond the plan: it asserts that the system prompt and the skill each say "management accounting" and "not a tax filing, statutory accounts or financial advice" (whitespace flattened), since ADR 0019 requires the role to say so. It was proved RED with the role files absent (the crate did not compile) and passes on the shipped wording.
- Task 1, a deviation decided by the controller: the plan said to show `kit_skills_name_only_tools_farik_lists` catching a role skill that names an unlisted tool "with a scratch line" in a skill. No scratch or fake tool name was written into any shipped `SKILL.md`, `system.md` or role file, not even temporarily. Instead a synthetic text, `("synthetic", "call farik_nope here")`, was pushed onto the test's own list of texts inside the test function; run alone it failed with `product_manager/synthetic: farik_nope`; the line was then removed.
