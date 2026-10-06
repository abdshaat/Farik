# Phase 7, step 07c: GitHub with a pasted token

Status: draft. Its readiness review (one round, ADR 0032) follows.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.1, 6.3, 6.7; F9
Depends on: steps 05 and 05b (the kit format, keys and `key_page`, the pins), 06 (the Product Manager's kit, at edd630f), 07 (the Architect's kit, at b24dabe and 4160280) and 03b (its table, empty before phase 11, and `ConnectorAdd`'s key fields; at 40e4480), all committed on this branch; it runs after step 08e's Task 8, as the project plan's ADR 0044 line orders, and shares no file with it; phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

Until Farik Cloud signs customers in to GitHub at the web launch (phase 11, ADR 0044), the Product Manager and the Architect can each be given GitHub's official server with a key the customer makes on GitHub and pastes (the founder, 2026-10-06: "Yes, pasted token"). "Get your key" opens GitHub's page with the key's name, life and permissions already filled in. The Product Manager reads issues and an organisation's project boards, turns an issue into a request, and files an issue or a comment only after the human allows that call. The Architect searches and reads the code and pull requests of the customer's own repositories, private ones included, and changes nothing there. Out of scope: signing in to GitHub (phase 11, whose single click is the founder's requirement); the Developer, which changes code only in its worktree, Farik integrating through git; anything that merges, pushes, deletes or changes a file on GitHub; any new screen.

## Decisions

- **No mockups.** GitHub connects through step 05's approved key form in `KitConnect` (about, why, setup, "Get your key", one field "Your GitHub key", Connect) and its Done list. This step adds data and prose.
- **The server and the key** (read 2026-10-06). GitHub's official remote server, `http`, `https://api.githubcopilot.com/mcp/`; its README (`github/github-mcp-server`, main) sends a key as `"Authorization": "Bearer ${input:github_mcp_pat}"`. Each kit's entry is `headers: { Authorization: "Bearer {GITHUB_KEY}", … }`, `credential_keys: [GITHUB_KEY]`, a `key_page`, and no `oauth`. A fine-grained key: GitHub's changelog of 2026-01-28 and the server's `docs/scope-filtering.md` say that with one "all tools shown, API enforces permissions", so the listing is the same for every customer's key and a pin can hold. Rejected: a classic key (it reaches every repository the person can, and the server hides tools by its scopes, so each customer's listing differs); the local server (`ghcr.io/github/github-mcp-server`, Docker, which no kit may start, ADR 0036).
- **The sign-in route falls back to the key by having none.** The entry names no `oauth`, so `team.get`'s kit row says `auth: keys` and `KitConnect` shows the key form; nothing signs in, so step 03b's table is never consulted. The format cannot offer both (`oauth_with_keys`, `oauth_header_conflict`), and a sign-in would answer `sign_in_not_supported`, GitHub registering no client by itself. A custom server at that address already falls to the key fields (03b's amendment by ADR 0044).
- **Narrowing the server, by headers** (`docs/remote-server.md`, read 2026-10-06: `X-MCP-Toolsets`, whose unknown names "are silently ignored"; `X-MCP-Readonly`, "only read tools"; `X-MCP-Tools`; `X-MCP-Lockdown`; `X-MCP-Insiders`). The Product Manager's entry sends `X-MCP-Toolsets: issues,projects`; the Architect's sends `X-MCP-Toolsets: repos,pull_requests` and `X-MCP-Readonly: "true"`. Listing (`filled_headers`) and the session's launch route (`launch_headers`, `connectors.rs:643`) both send every header the entry holds (read in the code, 2026-10-06), so the narrowing holds at connect, in the pin run and in sessions. Rejected: `X-MCP-Tools` naming exactly the tagged tools, because GitHub says an invalid tool name makes the server fail to start, so one rename at GitHub would stop every customer's GitHub until a Farik release, whereas a toolset that loses a tool only drops it, which the pin run reports; a path such as `/mcp/x/issues/readonly`, which names one toolset only.
- **No lockdown header** (O3). Lockdown hides "public issue details created by users without push access", which are the outside users' reports the Product Manager imports, and GitHub calls it "a best-effort content filter, not a security boundary". Every connector answer is already untrusted content under spec 8.6.
- **Three fences.** GitHub refuses what the key may not do; the headers keep other toolsets off the listing; the tags decide what is offered (a `denied` tool, and an untagged one, `tool_not_tagged`, ADR 0036, are never offered). So nothing that deletes, administers, touches secrets or workflows, merges, pushes or changes a file is offered: `repos`' and `pull_requests`' writes are hidden by the Architect's read-only mode and are in no toolset the Product Manager asks for, and `actions`, `secret_protection`, `git` and the other toolsets are asked for by neither kit. If GitHub lists one anyway, it is untagged, never offered, and the pin run reports it, the mechanical rule tagging it `denied`. Pins equal the narrowed listing, since `pin_drift` compares both ways, so the Architect's kit names no write tool.
- **What each tag is.** A read is `network`. Filing an issue and commenting are `external_effect`. `denied`: a tool that changes another issue's structure, edits a comment, or writes to a project board, which the founder's brief does not ask for; and `list_repository_collaborators`, a list of who may change a repository, which the Architect never needs. `issue_write`'s `update` method also edits, closes and reassigns, and a kit tags a tool, not an argument (as step 08 found for Higgsfield's voice-over); `ToolApproval` shows the whole input, the method included, and the skill limits it.
- **The Product Manager writes to GitHub** (O1). Filing an issue and commenting are the founder's brief for this step, but they reverse step 06's rule that the Product Manager's kit only reads (`docs/design/role-kits.md`, spec 6.1), which nothing the founder said in ADR 0044 decides; O1 asks, and the plan builds them, Task 5 changing the spec and the design.
- **No allowance** (O1). An issue or a comment is published to everyone who can see the repository, the world on a public one, and the kits' rule is that a tool that publishes or posts always asks (`docs/design/role-kits.md`). Step 10h (ADR 0041) rewords any kit copy that says a call always waits.
- **What a fine-grained key reaches** (GitHub's "Managing your personal access tokens", read 2026-10-06): one resource owner, the person's account or one organisation (O4); for an organisation that requires approval, the key is `pending` and reads public resources only until an administrator approves it (an owner's own is approved at once); not Projects owned by a user account (a documented gap), so the Product Manager reads an organisation's boards only, with the organisation permission Projects read; at most 50 keys per person; every key reads all public repositories.
- **The minimum permissions.** The Product Manager: repository Issues, read and write; and, when an organisation owns the boards, organisation Projects, read. The Architect: repository Contents, read, and Pull requests, read. Metadata, read, comes with any repository permission.
- **`key_page` is GitHub's template address** (the same page: `name` at most 40 characters, `description`, `target_name`, `expires_in` 1 to 366 or `none`, and `<permission>=<level>`, "validated by the token generation form"). The kit schema allows a query (`^https://`, 2048), and `key_page` is not word-checked (`kit.rs` checks only the copy, the labels and an allowance's `what`). Neither address sets `target_name`, since the customer's owner is unknown, nor `organization_projects`, since an organisation permission needs an organisation owner, which the form checks, and most customers' owner is their account: the setup says to add it. Both set `expires_in=366` (O2).
- **Copilot.** GitHub's page "Using the GitHub MCP Server" (docs.github.com, read 2026-10-06) says "The GitHub MCP server is available to all GitHub users regardless of plan type"; "Access to Copilot" is a prerequisite only in its sections for Copilot's editors (Visual Studio, JetBrains, Xcode, Eclipse), which Farik is not; and the server's `docs/policies-and-governance.md` says an organisation's "MCP servers in Copilot" policy governs Copilot's editors, while a third-party host with a key is governed by the organisation's key policies. So the setup says nothing about Copilot. A key GitHub refuses, for that or any reason, shows `KitConnect`'s existing words, "Farik could not connect GitHub. Check the key against GitHub’s page, and try again." Task 6's check uses an account that never turned Copilot on; if GitHub refuses it, the setups gain "GitHub asks for its Copilot plan for this; the free one is enough." in one commit, `fix(roles): say GitHub asks for Copilot`, and both setup tests check the sentence.
- **The key's name and the live variable.** `GITHUB_KEY`, as SerpApi's and Render's drafted entries name theirs. `live_kit_pins.rs` reads `FARIK_KIT_<NAME>_<KEY>` for a keyed entry and `_BEARER` only for one that signs in, so the founder sets `FARIK_KIT_GITHUB_GITHUB_KEY`, once for both kits (one connector name); `FARIK_KIT_GITHUB_BEARER` is the variable from phase 11. Any valid fine-grained key lists every tool, so one that reaches public repositories only is enough for the pin run.
- **`github` in both kits.** The two entries share the address and the key's name and differ in headers, `key_page`, tools, labels, `why` and `setup`, so they hash differently: a team file that gives an Architect the Product Manager's wider entry fails `matches_kit` (ADR 0036).
- **Removing it.** Remove deletes the key from the keychain (step 01), as for every keyed service; the key still works at GitHub until it ends or the customer deletes it there. No new words; phase 11's sign-in brings 03b's settings link.
- **The skills.** `using-product-sources` and `using-architecture-sources` learn GitHub (Task 3); the first's section 4, "You only read", becomes "You ask before you write". No new skill.
- **Pins**, by step 06's mechanical rule, unchanged: a tool the service lists and this plan lacks goes in `denied` with no label; a named tool the service no longer lists is removed only if the README fetched that day no longer names it; the tests follow in the same commit, recorded in the Execution notes. The lists below are the README's tools of each toolset (main, read 2026-10-06); the Architect's are those whose snapshot in `pkg/github/__toolsnaps__/<tool>.snap` (main, read 2026-10-06) has `readOnlyHint: true`, which read-only mode keeps, and all 30 of `repos` and `pull_requests` were checked: the 16 below are true, the 14 writes false. The Product Manager's tags agree with the same snapshots (its 8 reads true; `issue_write`, `add_issue_comment` and the 3 `denied` false). The founder's run (Task 6) confirms both lists.
- **When phase 11 arrives** (not planned here): each `github` entry signs in through Farik Cloud. It loses `headers.Authorization`, `credential_keys` and `key_page`, gains `oauth: {}` (the table's GitHub entry, matched by host), keeps its `X-MCP-*` headers (`launch_headers` replaces only `Authorization`), and its setup says to sign in; its hash changes, so every agent connected with a key sees "Connect again"; the live variable becomes `FARIK_KIT_GITHUB_BEARER`. Farik's GitHub App must then ask for Issues read and write and organisation Projects read, wider than the read-only permissions of 03b's Task 7 and ADR 0035, or the Product Manager's two writes stop at the switch. Whether a key stays offered beside the sign-in is phase 11's brainstorm.

For the founder (none blocks a task; each recommendation is what the plan does):
- **O1. Does the Product Manager file issues and comments on GitHub at all**, each asking the human with no allowance, or does GitHub stay read-only for it, as Amplitude, Linear and Notion are? Recommended: it files them, each asking, as the plan builds; this changes step 06's read-only rule and spec 6.1. If the founder answers read-only, Task 1 tags `issue_write` and `add_issue_comment` `denied`, drops `issues=write` for `issues=read` in `key_page`, ends `why` and `setup` with "It only reads.", and Task 3 keeps section 4 as "You only read".
- **O2. How long the key lasts.** Recommended: 366 days, GitHub's longest dated life; GitHub emails before it ends, and the customer pastes a new one through Remove and Connect. Rejected: 90 days (four renewals a year), no end (GitHub warns against it and organisations may forbid it).
- **O3. Lockdown.** Recommended: off, as above.
- **O4. One owner per key.** A customer whose code sits under both their account and an organisation connects GitHub for one of them per agent until phase 11. Recommended: accept; GitHub's App installation lifts it then.
- **O5. The Copilot check's account.** It needs a GitHub account that never turned Copilot on. Recommended: a free account made for the check, with one private test repository, which Task 6 also uses.

## File map

```
crates/roles/roles/product_manager/kit.yaml                                  modifies: github after notion, header comment (Task 1)
crates/roles/roles/architect/kit.yaml                                        modifies: github after osv, header comment (Task 2)
crates/roles/src/kit.rs                                                      tests (Tasks 1 to 3)
crates/roles/roles/product_manager/skills/using-product-sources/SKILL.md     modifies (Task 3)
crates/roles/roles/architect/skills/using-architecture-sources/SKILL.md      modifies (Task 3)
crates/runtime/src/daemon/team.rs                                            tests: connect by name (Task 4)
crates/runtime/tests/live_kit_pins.rs                                        modifies: header comment (Task 4)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md           modifies (Task 5)
docs/plans/phase-7-role-kits/step-07c-github-with-a-pasted-token.md          modifies: Execution notes, Status (Task 6)
```

## Interfaces

Consumes: `load_kit`, `KitConnector`, `SetupCopy`, `pin_drift`, `check_skill` (`farik-roles`, step 05); the `kit.rs` test helpers `service`, `pm_service`, `tagged`, `network_names` (steps 06, 07); `kit_entry`, `matches_kit` (`farik_runtime::daemon::team`, step 05); `custom_server`, `CustomTransport`, `ConnectorTag` (`farik-core`); `launch_headers` (`farik_runtime::connectors`, step 01), unchanged; `live_kit_pins_hold` (step 05).

Produces: no new signature. Data: two `github` kit entries and two skills' new text.

## Tasks

### Task 1: GitHub for the Product Manager

Files: `product_manager/kit.yaml` (`github` after `notion`; the header comment says four services, three that only read and GitHub, which asks before it writes); `kit.rs` tests.

**`github`**, `transport: http`, `url: https://api.githubcopilot.com/mcp/`, `headers: { Authorization: "Bearer {GITHUB_KEY}", X-MCP-Toolsets: "issues,projects" }`, `credential_keys: [GITHUB_KEY]`, `key_page: https://github.com/settings/personal-access-tokens/new?name=Farik+Product+Manager&description=Farik%27s+Product+Manager+reads+issues+and+files+the+issues+and+comments+you+allow.&expires_in=366&issues=write`. Title "GitHub". About "GitHub keeps your code and the work around it: issues, pull requests and project boards." Why "So the Product Manager can turn an issue you already wrote into a request, read your project boards, and file an issue or a comment when you say yes." Setup "“Get your key” opens GitHub's page with the key's name, a year's life and what it may do already filled in. Under ‘Resource owner’, choose the account or organisation that owns your issues; under ‘Repository access’, choose ‘Only select repositories’ and pick them. To read an organisation's project boards, also give ‘Projects’ read access. Choose ‘Generate token’ and paste the key here; an organisation's owner may need to approve it first. Farik asks you before each issue or comment it posts." (497 characters; apostrophes are ASCII, labels in ‘…’.)
- `network` (8): `issue_read` "read an issue and its comments", `list_issues` "list issues", `search_issues` "search issues", `list_issue_types` "list issue types", `list_issue_fields` "list issue fields", `get_label` "read a label", `projects_list` "list project boards and their cards", `projects_get` "read a project board or a card".
- `external_effect`, no allowance (2): `issue_write` "file or change an issue", `add_issue_comment` "comment on an issue".
- `denied` (3): `sub_issue_write`, `update_issue_comment`, `projects_write`.

Tests (`kit.rs`):
- `github_for_the_product_manager_files_issues_only_when_asked`: `pm_service("github")` is `http` at exactly that address, with no `oauth`; `credential_keys` exactly `["GITHUB_KEY"]`; `headers` exactly the two above (no `X-MCP-Readonly`, no `X-MCP-Lockdown`); `key_page` exactly the address above; `network_names` exactly the 8; `issue_write` and `add_issue_comment` the only `external_effect`, with no entry in the kit's own allowances map (read from the `KitConnector::Server`'s `allowances`, as `marketing_service` does); the 3 `denied` exactly; 13 tools; the label map's keys exactly the 8 and the 2. RED: no such connector.
- `the_product_managers_kit_asks_before_it_writes` replaces `the_product_managers_kit_only_reads` (`kit.rs:1511`): the connectors are exactly `amplitude, linear, notion, github`; no connector has allowances; `amplitude`, `linear` and `notion` keep every assertion the old test made (no `external_effect`, `oauth`, no keys, no headers); the kit's only `external_effect` tools are `github`'s two. RED: the kit has three connectors.
- `loads_every_shipped_kit`: the Product Manager has 4.

- [ ] `feat(roles): give the Product Manager GitHub with a pasted key`

### Task 2: GitHub for the Architect

Files: `architect/kit.yaml` (`github` after `osv`; the header comment names GitHub, a pasted key, read-only); `kit.rs` tests.

**`github`**, `transport: http`, `url: https://api.githubcopilot.com/mcp/`, `headers: { Authorization: "Bearer {GITHUB_KEY}", X-MCP-Toolsets: "repos,pull_requests", X-MCP-Readonly: "true" }`, `credential_keys: [GITHUB_KEY]`, `key_page: https://github.com/settings/personal-access-tokens/new?name=Farik+Architect&description=Farik%27s+Architect+reads+code+and+pull+requests+and+changes+nothing.&expires_in=366&contents=read&pull_requests=read`. Title "GitHub". About as Task 1's. Why "So the Architect can search and read the code and pull requests in your own repositories, private ones included, before it writes a decision. It only reads." Setup "“Get your key” opens GitHub's page with the key's name, a year's life and read-only access to code and pull requests already filled in. Under ‘Resource owner’, choose the account or organisation that owns your code; under ‘Repository access’, choose ‘Only select repositories’ and pick the ones the Architect may read. Choose ‘Generate token’ and paste the key here; an organisation's owner may need to approve it first. The Architect only reads." (446 characters.)
- `network` (15): `search_code` "search code", `get_file_contents` "read a file or folder", `list_branches` "list branches", `list_commits` "list changes", `get_commit` "read a change", `search_commits` "search changes", `list_tags` "list tags", `get_tag` "read a tag", `list_releases` "list releases", `get_latest_release` "read the latest release", `get_release_by_tag` "read a release", `search_repositories` "search repositories", `list_pull_requests` "list pull requests", `pull_request_read` "read a pull request", `search_pull_requests` "search pull requests".
- `denied` (1): `list_repository_collaborators`.

Tests (`kit.rs`):
- `github_for_the_architect_reads_code_and_pull_requests_only`: `service(Role::Architect, "github")` is `http` at that address, with no `oauth`; `credential_keys` exactly `["GITHUB_KEY"]`; `headers` exactly the three above; `key_page` exactly the address above; `network_names` exactly the 15; `list_repository_collaborators` the only `denied`; no `external_effect`; 16 tools. RED: no such connector.
- `osv_is_farik_s_own_server_and_only_reads` (`kit.rs:1456`): the connectors are exactly `context7, grep, osv, github`; its loop (no `external_effect`, no allowances) now covers `github`.
- `the_two_github_entries_differ_where_the_roles_do` (a guard): the two entries have the same address, `credential_keys` and `about`; their `custom_server`s differ; both `key_page`s start `https://github.com/settings/personal-access-tokens/new?`, and the Product Manager's alone holds `issues=write`; the Architect's alone holds `X-MCP-Readonly`.
- `loads_every_shipped_kit`: the Architect has 4. `every_network_tool_of_the_architect_has_a_label` is unchanged and must pass: the labels are the 15 `network` names exactly, the `denied` one unlabelled.

- [ ] `feat(roles): give the Architect GitHub to read code and pull requests`

### Task 3: The two skills learn GitHub

Files: the two `SKILL.md`; `kit.rs` tests. Each stays under 6 KB, with numbered sections, no `` !` ``, no `@` after a space, and only `farik_*` tools `tool_descriptors` lists (`kit_skills_name_only_tools_farik_lists`).

- `using-product-sources`, description "Use when Amplitude, Linear, Notion or GitHub is connected to the team, so that you read from them safely, use what they hold, and ask before you write to GitHub." Section 1 adds GitHub: an issue the user already wrote, with its comments (`issue_read`), to turn into a request; an organisation's project board (`projects_list`, `projects_get`), since a key cannot read a personal account's boards. Section 4 becomes "You ask before you write": Amplitude, Linear and Notion only read, as before; on GitHub, file an issue (`issue_write`, method `create`) or comment (`add_issue_comment`) only when the contract or the user asks for it; each call waits for the human, who sees it whole, so write it complete (the repository, the title, the body) before the call; never change, close or reassign an issue the user did not name; what you post on a public repository everyone can read, so never put a secret, a customer's data or an unannounced plan in it; a refused call (GitHub's "Resource not accessible") means the key does not reach that repository or that permission, or an organisation has not approved it yet: tell the user with `farik_ask_human`, and never try another way.
- `using-architecture-sources`, description "Use when Context7, Grep, OSV or GitHub is connected to you, so that you look things up safely and say what you used." The opening says four services. Section 1 adds GitHub: the user's own repositories the key reaches, private ones included; `search_code` searches default branches only, at most ten searches a minute (GitHub's limit), then `get_file_contents`; `pull_request_read` for a pull request's diff, files and review comments; the project you work on is already in your worktree, so read GitHub for the user's other repositories and for pull requests; it only reads. Section 2 says a GitHub search goes to GitHub, under the same rule.

- `the_product_managers_sources_skill_asks_before_it_writes_to_github`: the kit's `using-product-sources` `SKILL.md` names GitHub, `issue_write` and `add_issue_comment` and `farik_ask_human`, holds the heading "You ask before you write", and no longer holds "This kit has no way to change anything"; its description names GitHub. RED: the skill says the kit only reads.
- `the_architects_sources_skill_names_github`: `using-architecture-sources` names GitHub, `search_code` and `pull_request_read`, and its description names GitHub. RED: it names three services.

- [ ] `feat(roles): teach the Product Manager and the Architect to use GitHub`

### Task 4: Connected by name

Files: `daemon/team.rs` tests; `live_kit_pins.rs`'s header comment, which names GitHub in both kits and `FARIK_KIT_GITHUB_GITHUB_KEY`, a fine-grained key, which lists every tool whatever it may do (no code change: the run already lists every shipped `http` connector with its keys).

- `connects_every_shipped_kit_connector_by_name` (`daemon/team.rs:3950`, extended, a guard): for the Product Manager and for the Architect, `kit_entry` of `github` is `Ok` and `matches_kit` true against its own role's kit; `matches_kit` of the Product Manager's `github` against the Architect's kit is false, and the reverse; `kit_entry` of `github` on the Developer is `connector_not_in_kit`.

- [ ] `test(runtime): connect GitHub to the Product Manager and the Architect by name` (a guard, so no RED; it may be folded into Task 2's commit, said in the Execution notes)

### Task 5: Spec and plan

`docs/SPEC.md`: 6.1's Kit line, four services: three it only reads, and GitHub, which reads issues and an organisation's boards and files an issue or a comment only after the human allows the call; 6.3's Kit line drops "GitHub code search … is not in the kit" and adds GitHub, read only; 6.7's "The Product Manager's kit" paragraph drops "The kit only reads" for the three it names, and 6.7 gains "GitHub with a pasted key (added in 0.63; ADR 0044)": the address, the headers and why not `X-MCP-Tools`, `GITHUB_KEY` as a bearer, the two entries and their tags, no allowance, the three fences, the template page, what a fine-grained key reaches, Copilot as found, and that phase 11 replaces the key with a sign-in through Farik Cloud. The revision line takes the next number of its day (0.63 at planning). `docs/design/role-kits.md`: the Product Manager's and the Architect's rows say what shipped; the GitHub row of the Signing-in table drops "No kit ships GitHub yet; … open for the founder"; the Steps table gains 07c. `docs/plans/project-plan.md`: row 07c, what was executed.

- [ ] `docs(spec): record GitHub in the Product Manager's and the Architect's kits`

### Task 6: The founder's live run

Gate: Tasks 1 to 5 landed and landing-reviewed. No agent holds the founder's GitHub key or signs in to GitHub; the founder runs and reports, and the executor commits what the founder reports.

Founder's actions:
- [ ] **The pin run.** With `FARIK_KIT_GITHUB_GITHUB_KEY` set to a fine-grained key (one reaching public repositories only is enough) and every earlier kit's variable, `FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins`.
- [ ] **The web-app check**, with the GitHub account of O5 and one private test repository holding one issue and one open pull request. Open each kit's "Get your key"; record which fields GitHub filled (name, description, the year, each permission) and whether ‘Resource owner’, ‘Repository access’, ‘Only select repositories’, ‘Projects’ and ‘Generate token’ read so on the page. Connect a Product Manager with its key and a request that imports the issue; let it file one issue and one comment, each waiting on Today with its whole input, and allow both. Connect an Architect with its key; in one session it searches the repository's code with `search_code` and reads the pull request. Record whether the connects and calls worked without Copilot, and whether `search_code` returned the private repository's code.

Executor, from the report, each in its own commit with the tests that change with it: drift under the mechanical rule (`fix(roles): pin GitHub's tools from its live listing`); a quoted label that differs, replaced by GitHub's own, at most 60 characters on one line (`fix(roles): quote GitHub's page as it reads`); a template parameter GitHub did not fill, removed from `key_page` and its setup sentence corrected; the Copilot sentence of Decisions, if GitHub refused. Then the Execution notes record the report and Status says the run passed.

- [ ] `docs(plans): record GitHub's live run`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: test live_kit_pins_hold ... ok: the Product Manager's and the Architect's github listed with no drift, beside every earlier kit's
```

The step is not done until Task 6's run and check pass, or their drift is folded in under the mechanical rule, and the Execution notes record the result; Status stays "executed …; the founder's live run waits" until then.
