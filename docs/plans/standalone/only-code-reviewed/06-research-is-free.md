# Standalone 6 of 8: research is free

Status: draft
Branch: `fix/plans-need-no-review`
Spec: `docs/SPEC.md` 5.6 (the tier table, "A role held to read paths", "A role held to approved sites", "Read roots"), 5.17, 6.5, 6.6, 6.8, 6.10, 8.2, 8.3, 8.6; F9
Depends on: plans 1 to 5 committed on this branch; ADR 0061
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies (ADR 0008). One test per major functionality (ADR 0060). Tests that pin a removed limit change in the task that removes it, never skipped. The README's table lists every read limit removed or kept.

Line numbers are those of `74b4a5af`; plans 1 to 5 move some of them, so each item is found by the name given beside it.

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
  - Explore and chat sessions are registered with `Read`, and `Network` when the agent holds it (`ask.agent.tiers(&team.permissions())` contains it), whether or not they have the browser (`session.rs:289`). An agent the team file took `network` from (`revokes`) gets no `Network` in any session, unless the browser adds it as today.
  - A `read_only` session's built-ins (`builtins_of`, `session.rs:1133`) add `WebFetch` and `WebSearch` when the agent holds `network`.
- **The network in every sandbox.** `sandbox_for` (`crates/runtime/src/orchestrator.rs:643`) turns the network on when the agent holds `network`, with no `web_access` condition. `crates/runtime/src/sandbox/docker.rs`'s `none` network (line 127) stays for an agent the user took `network` from.
- **The Marketing Specialist's read limit goes.**
  - `ReadAccess`, `read_access`, `ReadAccess::allows` and `named` go, as does `folders_line`'s "Read only …" sentence (`crates/core/src/folders.rs:601-652`).
  - `SessionRegistration::reads` (`crates/runtime/src/daemon.rs:129`) goes, with `reads_of` and every place that sets it (`session.rs:304`, `daemon/mcp.rs`, `daemon/fixtures.rs`, `daemon/ads_calls/fixtures.rs`, `orchestrator/human.rs`, `orchestrator/rules.rs`, and `crates/cli/tests/{hook,serving,live_claude,connector_run}.rs`). So do `hooks::judge_reads` (`hooks.rs:420`) and every `read_not_allowed` refusal, and the words of `decide_pre_tool_use`'s doc comment (`hooks.rs:120-134`) that name them.
  - `members_read`'s exceptions for the Marketing Specialist and for a private-folder task go (`session.rs:112-116`). Both now get every other member's read root.
  - `marketing_specialist/system.md:27` says it reads the whole project.
- **The approved sites go.**
  - In `crates/core/src/governor/sites.rs`, `WebAccess`, `web_access`, `SiteRefusal`, `check_site_urls` and their helpers go. `site_of` and `SiteFault` stay, with their tests: they read an address's host for seller messages (`tools/seller.rs`, `procurement/mail.rs`), data pipeline requests (`tools/pipeline.rs`) and the waiting rows (`store/src/waiting.rs`, `daemon/gates.rs`). The module's doc comment says so.
  - `SessionRegistration::web` (`daemon.rs:125`) goes, with `web_of` and every place that sets it (the same files as `reads`).
  - In `hooks.rs`, `judge_fetch`, `judge_sites_of_call`, `approved_now` and `how_to_ask` go, with every `site_not_approved` and `sites_unreadable`.
  - In `crates/runtime/src/tools/sites.rs`, `request_sites`, `read_sites`, `approved_set`, `site_list`, `refuse_while_waiting` (and its call in `tools/work.rs:177`), `waiting_hosts`, `catervas_hosts` and their inputs and tests go. `shown`, which `tools/seller.rs`, `tools/pipeline.rs`, `procurement.rs` and `procurement/mail.rs` use to cut an address, stays, and is the module's one item.
  - `catervas_request_sites` and `catervas_read_sites` go from `tools.rs` and from the session's offer (`session.rs:1081`).
  - `crates/roles/src/sites.rs` (`catervas_sites`, `parse_catervas_sites`, `CatervasSite`, `SitesError`), `crates/roles/roles/procurement_specialist/approved_sites.yaml`, `docs/schemas/approved-sites.schema.json` and its generated module (`approved_sites` in `crates/roles/src/generated/mod.rs`) go; `kit.rs:1762`'s comment stops naming the approved-sites check.
  - `crates/store/src/sites.rs` goes (`approved_sites`, `site_requests`, `declined_sites`, `site_list` and their types), and `store/src/waiting.rs` lists no site request (line 635). The table of migration `0014_site_requests.sql` stays, since migrations are never rewritten, and nothing reads it.
  - The query `sites.list` (`daemon/gates.rs:312`) goes, from `docs/schemas/rpc.schema.json` too.
  - The CLI's `catervas site` (`Commands::Site` and `SiteCommands`, `crates/cli/src/lib.rs` 537, 838, 1250 to 1253 and 1698 to 1715, and `crates/cli/src/site.rs`) goes.
  - `purchase_order_site_not_approved` goes; `purchase_order_url_invalid` (an https address) stays.
  - `site_decide`, `site_add` and `site_remove` (`human.rs` 894, 973, 998) answer `REFUSED` with `sites_gone: Catervas no longer keeps a list of sites (ADR 0061)`. Their schemas stay, so old clients parse.
  - `site.requested` and its decisions in older logs still load, and nothing lists them.
  - The web's `Sites.tsx`, `dialogs/SiteRequest.tsx`, the agent page's sites line (`AgentEdit.tsx:461`), the waiting row for a site and their strings in `en.ts` (around line 2097) go.
  - The Procurement Specialist's `system.md:14` and every skill of its that names approved sites, `catervas_request_sites` or `catervas_read_sites` stop speaking of them and say it may open any page: `sourcing-a-product`, `checking-a-seller`, `checking-a-used-vehicle`, `checking-product-safety`, `finding-sellers-and-makers`, `writing-purchase-orders`, `requesting-a-data-pipeline`, `using-procurement-sources` and `contacting-sellers` (`grep -rln "site" crates/roles/roles/procurement_specialist/` lists them).
- **The private workbooks are open to read.**
  - `ReadSheetInput::folder` (`sheets.rs:829`) is `Option<OtherFolder>`, and `OtherFolder` gains `Finance` (wire `finance`) beside `Procurement`. Its description says: name `finance` or `procurement` to read that private folder's workbook; leave it out to read your own folder, or, in a session about a task in a private folder, that task's.
  - `folder_to_read` (`crates/runtime/src/tools/sheets.rs:866`) answers, in order: the folder `folder` names, for any role; else, in any session about a task whose assignee's role has a private folder, that folder; else the caller's own private folder; else `sheet_refused` "name `folder`: finance or procurement". The procurement folder's `REGISTER`-only rule (`sheets.rs:1052`) goes: any workbook of either folder is read.
  - `baseline` reads a task's copy under `.history/<task>/` as today, for a session about a task in that folder.
  - `catervas_read_sheet` is offered in every session (`session.rs:1095`).
  - `private_path_refused` stays for paths outside a folder or through a link (`private_path`), so no read reaches the rest of `.catervas/local/`. Writes stay the owner's.
  - A private-folder task's session stays confined to its folder (`workspace_paths`), since that is where it writes. `.catervas/local/**` stays protected for built-in reads, because it also holds connector keys.

## File map

```
crates/core/src/governor/permissions.rs, crates/runtime/src/orchestrator/session.rs, orchestrator.rs, claude.rs   network everywhere (Task 1)
crates/core/src/folders.rs, crates/runtime/src/daemon.rs, daemon/hooks.rs, daemon/mcp.rs, daemon/fixtures.rs, daemon/ads_calls/fixtures.rs, orchestrator/session.rs, orchestrator/human.rs, orchestrator/rules.rs, crates/cli/tests/{hook,serving,live_claude,connector_run}.rs, crates/roles/roles/marketing_specialist/system.md   reads open (Task 2)
crates/core/src/governor/sites.rs, crates/runtime/src/daemon.rs, daemon/hooks.rs, daemon/mcp.rs, daemon/fixtures.rs, daemon/ads_calls/fixtures.rs, daemon/gates.rs, tools/sites.rs, tools/work.rs, tools/purchase_order.rs, tools/pipeline.rs (tests), tools.rs, orchestrator.rs, orchestrator/session.rs, orchestrator/human.rs, orchestrator/rules.rs, crates/roles/src/sites.rs (deleted), crates/roles/src/lib.rs (its mod line), crates/roles/src/generated/mod.rs, crates/roles/src/kit.rs, crates/roles/roles/procurement_specialist/**, docs/schemas/approved-sites.schema.json (deleted), docs/schemas/rpc.schema.json, crates/store/src/sites.rs (deleted), crates/store/src/lib.rs (its mod line), crates/store/src/waiting.rs, crates/cli/src/lib.rs, crates/cli/src/site.rs (deleted), crates/cli/tests/{human,hook,serving,live_claude,connector_run}.rs, apps/web/src/pages/Sites.tsx, dialogs/SiteRequest.tsx, AgentEdit.tsx, Today.tsx, apps/web/src/strings/en.ts   sites go (Task 3)
crates/runtime/src/tools/sheets.rs, orchestrator/session.rs                       workbooks open (Task 4)
```

## Interfaces

Consumes: `default_tiers`, `PermissionTier::Network`, `sandbox_for`, `members_read`, `folder_to_read`, harnesses of `hooks.rs` (`READABLE` and the session registration helpers), `sheets.rs`, `rules.rs`.
Produces: `OtherFolder::Finance` (wire `finance`) on `catervas_read_sheet`'s `folder`. Removed: the items in Decisions.

## Tasks

### Task 1: every role and every session may search and fetch

Files: those of the File map for Task 1. These tests change here:
- `permissions.rs`: `gives_each_role_the_default_tiers_of_the_spec_table` (1114), `finance_reads_and_researches_only` and `procurement_reads_and_researches_only`;
- `claude.rs:1657`;
- `rules.rs`: `gives_a_chat_the_read_tier_alone` (9476), `denies_a_chat_everything_else` (9658) and `offers_a_verify_session_no_tool_that_runs_or_writes` (6110);
- `rules.rs`'s `gives_a_role_held_to_approved_sites_a_sandbox_with_no_network` (5839), which becomes `gives_every_role_s_sandbox_the_network`;
- `crates/runtime/src/orchestrator/design.rs`' `holds_the_explore_session_to_the_read_tier` (396, written above as `design.rs:396`).

Tests:
- `every_role_and_session_may_research` (`rules.rs`; ignored, needs git):
  - A Developer's implement session, a Designer's explore session, a chat and a verify session each offer `WebFetch` and `WebSearch` in their `--tools`, and the hook lets a `WebFetch` of `https://example.com/` through in each.
  - The Developer's task's sandbox is made with the network.
  - A Developer the team file took `network` from (`revokes: [network]`) is offered neither tool in its implement session or its chat, the hook denies its `WebFetch`, and its sandbox has no network.

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

Files: those of the File map for Task 3. These tests are deleted with the code: those of `sites.rs` (core) but `site_of`'s; those of `tools/sites.rs` but `shown`'s; `store/src/sites.rs`'; `human.rs`' `allowing_a_request_approves_its_site` (5823), `the_owner_adds_a_site_unasked` (6014) and `the_owner_turns_off_a_catervas_site` (6092); `session.rs`' `offers_the_site_tools_to_a_held_role` (3238); `rules.rs`' `starts_a_procurement_session_held_to_approved_sites` (9750); the site rows of `gates.rs` (5040, 5080, 5866) and of `store/src/waiting.rs`; `crates/cli/tests/human.rs`' site test (1544); the `kit.rs` and `lib.rs` tests of `approved_sites.yaml`; and the web tests of the removed pages (`sites.test.tsx`). The `hooks.rs` site tests (3720 to 4169) are deleted, except the one this task turns. `purchase_order.rs`'s `refuses_a_page_not_on_an_approved_site` becomes `takes_any_https_page`, and its use of `catervas_sites()` (730) goes. `tools/pipeline.rs`' tests that read `approved_set` (1092, 1126) stop asserting that no site was approved.

Tests:
- `procurement_fetches_any_site` (`hooks.rs`, replacing `procurement_fetches_only_approved_sites`):
  - A Procurement Specialist's `WebFetch` of `https://shop.example/item` is allowed.
  - A `serpapi` call whose input holds `https://other.example/` is allowed by the sites (a `paid` call still asks, plan 5).
  - `site_decide` answers `REFUSED` with the message starting `sites_gone: `.

- [ ] `feat(runtime): let the Procurement Specialist fetch any site`

### Task 4: every role reads the private workbooks

Files: those of the File map for Task 4. `sheets.rs` tests `other_roles_never_reach_the_register` (2690) and `finance_reads_the_register_and_nothing_else_there` (2630) become the test below; `the_register_is_never_read_as_a_copy` and `refuses_a_link_at_or_above_the_folder` stay, the second being the guarantee that no read leaves the folder for the rest of `.catervas/local/`.

Tests:
- `any_role_reads_either_private_folder` (`sheets.rs`):
  - A Product Manager's session reads `books.xlsx` with `folder: finance` and `vendors.xlsx` with `folder: procurement`.
  - A Developer's session reads `comparisons/laptops.xlsx` with `folder: procurement`.
  - A Developer's read with no `folder`, in a session about no task, answers `sheet_refused`, and a `path` of `../settings.yaml` with `folder: finance` answers `private_path_refused`.
  - Writing either still answers `sheet_refused` for any role but the owner.

- [ ] `feat(runtime): let every role read the private workbooks`

## Verification

```
cargo xtask check
# expected: xtask check: ok
cargo xtask check --integration
# expected: xtask check: ok
```
