# Standalone 6 of 8: research is free

Status: draft
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 5.6 (the tier table, "A role held to read paths", "A role held to approved sites", "Read roots"), 5.17, 6.5, 6.6, 6.8, 6.10, 8.2, 8.3, 8.6; F9
Depends on: plans 1 to 5 committed on this branch; ADR 0061
Readiness confirmed by: pending

Signatures, not bodies (ADR 0008). One test per major functionality (ADR 0060). Tests that pin a removed limit change in the task that removes it, never skipped. The README's table lists every read limit removed or kept.

## Goal

Any agent may research anything:
- Every role holds `network`, and every session may use `WebFetch` and `WebSearch`.
- Every task's sandbox has the network.
- The Marketing Specialist reads the whole project, and every member of a project folder.
- The Procurement Specialist fetches any site.
- Every role may read the Finance and Procurement workbooks through `catervas_read_sheet`.

Secrets stay out of reach, unchanged: the protected paths (`.env`, `.env.*`, `**/*.pem`, `**/*.key`, `.catervas/local/**` and the team's own), the keychain, the model credential, connector keys and OAuth tokens, `~` expansion, and `Bash`. Writes stay as they are: allowed paths, folder owners, and private folders written by their owner alone.

## Decisions

- **Every role holds `network`.**
  - `default_tiers` (`crates/core/src/governor/permissions.rs:39`) adds `Network` to the Software Developer, the UI/UX Designer and the Scrum Master. A tier the user took from an agent in `team.yaml` stays taken.
  - Explore and chat sessions are registered with `[Read, Network]` (`session.rs:289`), whether or not they have the browser.
  - A `read_only` session's built-ins (`builtins_of`, `session.rs:1133`) add `WebFetch` and `WebSearch` when the agent holds `network`.
- **The network in every sandbox.** `sandbox_for` (`crates/runtime/src/orchestrator.rs:661`) turns the network on when the agent holds `network`, with no `web_access` condition. `docker.rs`'s `--network none` stays for an agent the user took `network` from.
- **The Marketing Specialist's read limit goes.**
  - `ReadAccess`, `read_access`, `ReadAccess::allows` and `named` go, as does `folders_line`'s "Read only …" sentence (`crates/core/src/folders.rs:601-652`).
  - The registration's `reads` goes (`session.rs:304`), and so do `hooks::judge_reads` and every `read_not_allowed` refusal (`crates/runtime/src/daemon/hooks.rs:321-369`).
  - `members_read`'s exceptions for the Marketing Specialist and for a private-folder task go (`session.rs:112-116`). Both now get every other member's read root.
  - `marketing_specialist/system.md:27` says it reads the whole project.
- **The approved sites go.**
  - `WebAccess`, `web_access`, `site_of`, `check_site_urls` (`crates/core/src/governor/sites.rs`) go, and the module with them.
  - In `hooks.rs`, `judge_fetch`, `judge_sites_of_call`, `approved_now` and `how_to_ask` go, with every `site_not_approved` and `sites_unreadable`.
  - `catervas_request_sites` and `catervas_read_sites` (`crates/runtime/src/tools/sites.rs`, `tools.rs`, `session.rs:1081`) go, and so does `catervas_sites()` (`crates/roles/src/sites.rs`).
  - `purchase_order_site_not_approved` goes; `purchase_order_url_invalid` (an https address) stays.
  - `site_decide`, `site_add` and `site_remove` (`human.rs` 894, 973, 998) answer `REFUSED` with `sites_gone: Catervas no longer keeps a list of sites (ADR 0061)`. Their schemas stay, so old clients parse.
  - `site.requested` and its decisions in older logs still load, and nothing lists them.
  - The web's `Sites.tsx`, `dialogs/SiteRequest.tsx`, the agent page's sites line (`AgentEdit.tsx:461`), the waiting row for a site and their strings in `en.ts` (around line 2097) go.
  - The Procurement Specialist's `system.md:14` and its skills `sourcing-a-product`, `checking-a-seller` and `checking-a-used-vehicle` stop speaking of approved sites.
- **The private workbooks are open to read.**
  - `folder_to_read` (`crates/runtime/src/tools/sheets.rs:866`) lets any role read any workbook of either private folder, and its copies under `.history/<task>/` for that task.
  - `catervas_read_sheet` is offered in every session (`session.rs:1095`).
  - `private_path_refused` stays for paths outside a folder or through a link. Writes stay the owner's.
  - A private-folder task's session stays confined to its folder (`workspace_paths`), since that is where it writes. `.catervas/local/**` stays protected for built-in reads, because it also holds connector keys.

## File map

```
crates/core/src/governor/permissions.rs, crates/runtime/src/orchestrator/session.rs, orchestrator.rs, claude.rs   network everywhere (Task 1)
crates/core/src/folders.rs, crates/runtime/src/daemon/hooks.rs, orchestrator/session.rs, crates/roles/roles/marketing_specialist/system.md   reads open (Task 2)
crates/core/src/governor/sites.rs, crates/core/src/governor.rs (its mod line), crates/runtime/src/daemon/hooks.rs, tools/sites.rs, tools/purchase_order.rs, tools.rs, orchestrator/session.rs, orchestrator/human.rs, crates/roles/src/sites.rs, crates/roles/roles/procurement_specialist/**, crates/store/src/sites.rs, apps/web/src/pages/Sites.tsx, dialogs/SiteRequest.tsx, AgentEdit.tsx, Today.tsx, apps/web/src/strings/en.ts   sites go (Task 3)
crates/runtime/src/tools/sheets.rs, orchestrator/session.rs                       workbooks open (Task 4)
```

## Interfaces

Consumes: `default_tiers`, `PermissionTier::Network`, `sandbox_for`, `members_read`, `folder_to_read`, harnesses of `hooks.rs` (`READABLE` and the session registration helpers), `sheets.rs`, `rules.rs`.
Produces: nothing new. Removed: the items in Decisions.

## Tasks

### Task 1: every role and every session may search and fetch

Files: those of the File map for Task 1. These tests change here:
- `permissions.rs`: `gives_each_role_the_default_tiers_of_the_spec_table` (1114), `finance_reads_and_researches_only` and `procurement_reads_and_researches_only`;
- `claude.rs:1657`;
- `rules.rs`: `gives_a_chat_the_read_tier_alone` (9476), `denies_a_chat_everything_else` (9658) and `offers_a_verify_session_no_tool_that_runs_or_writes` (6110);
- `design.rs:396`;
- `rules.rs`'s `gives_a_role_held_to_approved_sites_a_sandbox_with_no_network` (5839), which becomes `gives_every_role_s_sandbox_the_network`.

Tests:
- `every_role_and_session_may_research` (`rules.rs`; ignored, needs git):
  - A Developer's implement session, a Designer's explore session, a chat and a verify session each offer `WebFetch` and `WebSearch` in their `--tools`, and the hook lets a `WebFetch` of `https://example.com/` through in each.
  - The Developer's task's sandbox is made with the network.
  - A Developer the team file took `network` from has neither.

- [ ] `feat(core): let every role and session research the web`

### Task 2: every role reads the whole project

Files: those of the File map for Task 2. These `hooks.rs` tests are deleted or turned to assert the open read: `the_marketing_specialist_reads_only_its_two_folders` (4269), `a_held_search_names_a_path_inside_the_folders`, `a_held_session_runs_no_command_and_diffs_only_its_own_task`, `a_link_out_of_a_folder_is_judged_where_it_points` and `other_roles_read_as_before`. So are the `folders.rs` tests at 1259, 1282 and 1314, and `rules.rs` 9803 and 9878.

Tests:
- `the_marketing_specialist_reads_the_whole_project` (`hooks.rs`, replacing 4269):
  - In a Marketing Specialist's session, a `Read` of `src/main.rs` and a `Grep` at the worktree's top are allowed.
  - A `Read` of `.env` is still denied `path_protected`.
  - In a project folder, the session is given every other member's read root.

- [ ] `feat(runtime): let the Marketing Specialist read the whole project`

### Task 3: the Procurement Specialist fetches any site

Files: those of the File map for Task 3. The tests of `sites.rs` (core), `tools/sites.rs`, `store/src/sites.rs` and `human.rs` 5823, 6014 and 6092 are deleted with the code. The `hooks.rs` site tests (3720 to 4169) are deleted, except the one this task turns. `purchase_order.rs`'s `refuses_a_page_not_on_an_approved_site` becomes `takes_any_https_page`. `session.rs:3238` is deleted, and so are the web tests of the removed pages.

Tests:
- `procurement_fetches_any_site` (`hooks.rs`, replacing `procurement_fetches_only_approved_sites`):
  - A Procurement Specialist's `WebFetch` of `https://shop.example/item` is allowed.
  - A `serpapi` call whose input holds `https://other.example/` is allowed by the sites (a `paid` call still asks, plan 5).
  - `site_decide` answers `REFUSED` with the message starting `sites_gone: `.

- [ ] `feat(runtime): let the Procurement Specialist fetch any site`

### Task 4: every role reads the private workbooks

Files: those of the File map for Task 4. `sheets.rs` tests `other_roles_never_reach_the_register` (2690) and `finance_reads_the_register_and_nothing_else_there` (2630) become the test below; `the_register_is_never_read_as_a_copy` and `refuses_a_link_at_or_above_the_folder` stay.

Tests:
- `any_role_reads_either_private_folder` (`sheets.rs`):
  - A Product Manager's session reads `books.xlsx` from the finance folder and `vendors.xlsx` from the procurement folder.
  - A Developer's session reads a procurement comparison workbook.
  - Writing either still answers `sheet_refused` for any role but the owner.

- [ ] `feat(runtime): let every role read the private workbooks`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
