# Phase 7, step 07: Architect and Developer kits (the Architect)

Status: draft
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.3, 6.7; F9
Depends on: steps 05, 05b and 06 of this phase (committed on this branch, at 834fb8e), and the steps they rest on (01 to 04b); phase 6 (merged in #19)
Readiness confirmed by: fresh-session Opus reviewer, 2026-10-02: not ready, 2 Blocking, all folded; no second round (ADR 0032)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). The row is split at the role: this plan is the Architect's kit and the one piece of plumbing it needs; `step-07b-developer-kit.md` is the Developer's, which reuses this step's Context7 entry.

## Goal

The Architect ships a real kit: five skills, the security review among them, and three services it only reads, each connected with nothing pasted. Context7 (library documentation, signed in), Grep (public code search, no account) and OSV (the open vulnerability database, through a small server of Farik's own, no account). The Architect can look a library up, see how other projects use it, and check a dependency for known flaws, in its document tasks and in its reviews, where it runs no commands. Out of scope: GitHub code search over the user's private repositories (it waits for step 03b's sign-in, O2); the Developer's kit (step 07b); any new screen.

## Decisions

- **No mockups.** Step 05's screens (the kit list on `AgentEdit`, `ConnectorAdd` from a kit, `KitConnect`) already show a kit service with no key and one that signs in; this step adds data, prose and one small server.
- **How each server was chosen**, by ADR 0020's order (the service's official server, else a pinned community one, else a thin one of Farik's) and ADR 0035's routes, preferring a remote server the user signs in to, then one that needs nothing. Researched on 2026-10-02. Context7's tools were listed at the keyless `/mcp` (the same server, 4.1.1); `/mcp/oauth` answers 401 until signed in, and the live pin confirms its list. Grep answered `initialize` and `tools/list` with no credentials:
  - **Library documentation: Context7, official, `http`, `https://mcp.context7.com/mcp/oauth`, signed in (route 1), `oauth: { scopes: [profile, email, offline_access] }`.** `/mcp/oauth` answers 401 with `resource_metadata` pointing at `https://mcp.context7.com/.well-known/oauth-protected-resource` (`resource` `https://mcp.context7.com`, `authorization_servers` `clerk.context7.com` then `context7.com`, `scopes_supported` `[profile, email]`). `clerk.context7.com`'s metadata has `registration_endpoint`, S256, `token_endpoint_auth_methods_supported` including `none`, `authorization_response_iss_parameter_supported: true`, and `offline_access` among its scopes; Clerk issues a refresh token only for `offline_access`, so it is asked for. Context7's page "OAuth for Context7 MCP Remote Server" (context7.com/docs/howto/oauth) names `/mcp/oauth`. The keyless `/mcp` is `serverInfo` "Context7" 4.1.1 and lists two tools, `resolve-library-id` and `query-docs`, both `readOnlyHint: true`. Its free plan allows 1,000 calls a month (context7.com/plans). Rejected: the keyless `https://mcp.context7.com/mcp`, which answers without any sign-in at a lower, unpublished limit (O1, the founder's choice); and `@upstash/context7-mcp@4.1.1` (npm, 2026-09-14), a local program that takes a pasted key and whose own dependencies float under `^` ranges, which an exact pin of the top package does not hold.
  - **Code search: Grep by Vercel, official, `http`, `https://mcp.grep.app`, no sign-in and no key.** The unauthenticated probe answered (`serverInfo` "mcp-typescript server on vercel" 0.1.0, `tools.listChanged: true`) and lists one tool, `searchGitHub`, over public GitHub code; its list may change without notice, so expect drift between live runs, which the pin test catches. What the Architect needs from code search is how other projects use a library or solve a problem: the project's own code it already reads. **GitHub code search waits for step 03b** (O2): GitHub's server (`api.githubcopilot.com`) offers no registration, and a pasted GitHub key goes against ADR 0035. When 03b lands, a later plan may add GitHub's read-only server to this kit for the user's private repositories.
  - **Vulnerability database: OSV, through a thin server of Farik's own, `stdio`, `command: farik`, `args: [connector, osv]`, no sign-in and no key.** No official server is usable. Google's `osv-scanner experimental-mcp` (`google/osv-scanner`, `cmd/osv-scanner/mcp/command.go`; v2.6.0, 2026-09-14) is a Go program no allowed runner starts (`npx`, `bunx`, `uvx`, `pipx run`), it is marked experimental, and its `scan_vulnerable_dependencies` takes any path, which a server running on the host as the user (spec 6.7) would read outside the sandbox. The community servers are each one maintainer's: `@cyanheads/osv-advisory-mcp-server@0.1.15` (npm, 2026-09-24) runs on the host with dependencies on `^` ranges, `osv-mcp@1.0.1` is three days old, Stacklok's `osv-mcp` is a Go binary or a container, and Pipeworx's is a third party's gateway. OSV's own API is public, keyless and three calls (`POST https://api.osv.dev/v1/query`, `POST /v1/querybatch`, `GET /v1/vulns/{id}`, all probed 2026-10-02), so Farik's server is about 200 lines, changes only with a Farik release, and sends OSV only a package's name, ecosystem and version, or an advisory's id.
- **Farik's own server is named `farik` in a kit** (ADR 0038, written by Task 3). A kit cannot know where Farik is installed, so the absolute-path rule of ADR 0036 cannot be met by a shipped kit. The loader accepts the bare `command: farik` (only that word, no path, no `farik-…`); the program that starts it, the daemon listing its tools and `farik connector run` launching it, runs its own executable for that word, never a `farik` found on `PATH`. The hash keeps `farik`, so the server's code changes only with the binary, which is ADR 0036's promise. Rejected: a separate `farik-osv` binary, which needs the same path rule and a second release artefact; Farik tools (`farik_*`), which are not optional and follow the agent's tiers, so the Developer, without `network`, could never have one. The loader accepts the bare `farik` only when `is_farik_connector` holds, so it never takes another `args`. `program` swaps in Farik's own executable only for that exact pair, whatever the entry's `source`; any other `farik` command, a user's own included, is run as given, found on `PATH` as SPEC 6.7 says for a bare name. When the own executable cannot be found (`current_exe` fails), listing and launching fail with `farik could not find its own program`; they never fall back to `PATH`. `farik-e2e-serve`'s `current_exe` is not `farik`, so no browser suite connects OSV through it; none does in these steps.
- **The OSV server** is `farik_runtime::osv`, an `rmcp` server over stdio (`rmcp`'s `transport-io` feature, added to the workspace's existing exact pin; no new crate). Its address is a constant, `https://api.osv.dev/v1`, never an argument, an environment value or a tool input; the tests pass a fixture's address to the function, not to the command. It implements `ServerHandler` by hand, as `daemon/mcp.rs` does (`rmcp` has no `macros` feature here; `server` is already on, and `transport-io` adds only `tokio/io-std`); its `serverInfo.name` is exactly `"farik-osv"`. It uses one `reqwest::Client` (the workspace's, with rustls) built with `redirect::Policy::none()`, no proxy from the environment (`no_proxy()`), no cookie store and no default headers but `content-type: application/json` on POSTs; it gives up after 25 seconds (OSV may take 20 before it pages); it reads the body with `Response::chunk()` until 4 MiB, as `sign_in.rs:199` does. An answer over 4 MiB is a tool error, "OSV's answer is too large; ask about one version of the package", sent nowhere else. Farik's server asks for one page only and never sends `page_token`. It runs with a cleared environment (`KEPT_ENV`), so it ignores `HTTPS_PROXY` and `SSL_CERT_FILE`; a user behind a proxy gets a tool error, accepted for now. It answers in plain JSON:
  - `query_package { ecosystem, name, version }`: each advisory's `id`, `summary`, `aliases`, `severity`, and `fixed`, the versions that fix it, from `ranges[].events[].fixed` of only the `affected` entries whose `package` matches the asked `ecosystem` and `name` (an advisory with none says `"fixed": []`); at most 50 advisories, in OSV's order, then `"more": true` when it cut the list or OSV gave a `next_page_token`, else `"more": false`; an answer holding only a `next_page_token` says `"more": true` with no advisories;
  - `query_packages { packages: [{ ecosystem, name, version }] }`, 1 to 100: per package, the advisory ids (`querybatch` gives no more), and `"more": true` for a result carrying a `next_page_token`;
  - `get_vulnerability { id }`: the record with `details` cut at 8,000 characters and `affected` cut to 20 entries, each cut said so in the answer.
  An `ecosystem` is 1 to 40 of letters, digits and `.:_ -`; a `name` 1 to 214 characters with no control character; a `version` 1 to 128 with none; an `id` 1 to 64 characters, the first a letter or a digit, then letters, digits and `-._:`, added to the address as one percent-encoded path segment (`Url::path_segments_mut().push`). Anything else is a tool error, sent nowhere. OSV's answer is data the agent reads under the untrusted-content notice (spec 8.6), as every connector's is.
- **What each tag is.** Every tool of the three is a read: all six are `network`, none `denied` or `external_effect`, and no allowance. Context7's two are `readOnlyHint`; Grep's one searches public code; OSV's three send a package's name and version and read advisories. The Architect has the `network` tier already, so none adds a way out it lacked; what the agent sends is its query, which the setup copy says. Grep's `query` goes to Vercel as written and searches public GitHub only, so no tool returns private data; the leak risk is the agent's own query, which the setup copy and `using-architecture-sources` cover. No label changes.
- **What a query reveals.** A search or a question goes to the service as the agent wrote it. The skill `using-architecture-sources` says never to paste the project's code or a secret into one; the copy says where the words go.
- **Kit skills are embedded** as step 06 did: `embedded_skills(Role::Architect)` returns the five `(name, &[("SKILL.md", include_str!(…))])` pairs. The role's own `reviewing-for-design` stays in the prompt; no kit skill repeats its decision-writing or review mechanics.
- **The copy** is given in full in Task 2. None of it says "MCP", "OAuth" or "token", and none quotes a label.
- **Pins against the live service**, by step 06's mechanical rule, unchanged: a tool the service lists and this plan lacks goes in `denied` with no label; a named tool the service no longer lists is removed only if the service's documentation fetched that day no longer names it; the counts and lists in this plan's tests follow in the same commit, recorded in the Execution notes. OSV's server is Farik's, so its pin is an offline test (Task 3), not a live one.
- **Context7's sign-in, a fallback.** Its resource names two authorization servers and `rmcp` uses the first. If the founder's sign-in through the web app fails, or gives no refresh token, the mechanical fallback is the keyless `https://mcp.context7.com/mcp` with no `oauth`, and the setup's first sentence becomes "Nothing to set up: Context7 needs no account for a few questions a day." It is recorded in the Execution notes, and Task 2's test changes with it, in its own commit, `fix(roles): reach Context7 without signing in`, which edits both kits once 07b exists.

Decided by the controller, 2026-10-02: **O1** Context7 by sign-in, with the keyless fallback in its own commit; **O2** Grep now, GitHub code search after 03b; **O3** OSV through Farik's own server, with ADR 0038 and `is_farik_connector` limited to the exact `connector osv`. The live pins need one account: a free Context7 account. Grep and OSV need none.

## File map

```
crates/roles/roles/architect/skills/<5 names>/SKILL.md     creates (Task 1)
crates/roles/roles/architect/kit.yaml                      modifies: skills (Task 1), connectors (Task 2, Task 4)
crates/roles/src/kit.rs                                    modifies: embedded skills (Task 1); bare `farik` (Task 3); tests (Tasks 1 to 4)
crates/runtime/src/connectors.rs                           modifies: `list_tools` runs Farik's own executable for `farik` (Task 3)
crates/runtime/src/daemon/team.rs                          modifies: passes the daemon's executable to `list_tools`; tests (Tasks 1, 3, 5)
crates/cli/src/connector.rs                                modifies: both `list_tools` calls pass `current_exe()` (Task 3)
crates/runtime/tests/fixture_mcp.rs, fixture_oauth.rs      modifies: `list_tools` callers pass `Path::new("farik")` (Task 3)
crates/cli/src/connector_run.rs                            modifies: runs its own executable for `farik` (Task 3)
crates/runtime/src/osv.rs                                  creates: the OSV server, its tests with a fixture (Task 4)
crates/cli/src/lib.rs                                      modifies: `farik connector osv` (Task 4)
crates/cli/tests/osv_server.rs                             creates: the binary lists the kit's tools (Task 4)
crates/cli/tests/connector_run.rs                          tests: the launcher starts its own executable for `farik` (Task 4)
Cargo.toml                                                 modifies: `rmcp` gains `transport-io` (Task 4)
crates/runtime/tests/live_kit_pins.rs                      modifies: new `list_tools` argument (Task 3); skips `farik`, its pin is offline (Task 5)
docs/decisions/0038-farik-s-own-connectors-in-a-kit.md     creates (Task 3)
docs/decisions/0036-…md                                    modifies: one-line amendment pointing at 0038 (Task 3)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md   modifies (Task 6)
```

## Interfaces

Consumes: `load_kit`, `parse_kit`, `Kit`, `KitConnector`, `SetupCopy`, `check_skill`, `pin_drift` (`farik-roles`, step 05); `kit_entry`, `matches_kit` (`farik_runtime::daemon`, step 05); `list_tools`, `LaunchSpec` (`farik_runtime::connectors`, step 01); `custom_server`, `CustomTransport`, `ConnectorTag` (`farik-core`); the test helpers `network_names` and `signed_in` (`kit.rs` tests, step 06).

Produces:

```rust
// farik_roles::kit
pub const FARIK_COMMAND: &str = "farik";
pub const FARIK_CONNECTORS: &[&str] = &["osv"];
pub fn is_farik_connector(command: &str, args: &[String]) -> bool; // command == "farik" && args == ["connector", n], n in FARIK_CONNECTORS
// farik_runtime::connectors
pub fn program(command: &str, args: &[String], farik: &std::path::Path) -> std::path::PathBuf; // farik when is_farik_connector, else command
pub async fn list_tools(server: &CustomServer, keys: &BTreeMap<String, Secret>, bearer: Option<&Secret>,
    folder: &Path, farik: &Path) -> Result<Vec<ListedTool>, ConnectorError>; // gains `farik`
// farik_runtime::osv
pub const OSV_API: &str = "https://api.osv.dev/v1";
pub async fn serve_stdio(api: &str) -> Result<(), OsvError>;
pub fn tool_names() -> Vec<&'static str>;
```

## Tasks

### Task 1: The Architect's skills

Files: `architect/skills/{designing-apis-and-data,reviewing-dependencies,security-review,setting-performance-budgets,using-architecture-sources}/SKILL.md`; `kit.yaml` `skills` in that order; `embedded_skills`' `Architect` arm. Each has frontmatter `name` and a `description` starting "Use when", numbered sections, under 6 KB, no `` !` `` line, no fence opening `!`, no attached `@` file. Every `farik_*` a skill names is one `tool_descriptors` lists (`kit_skills_name_only_tools_farik_lists` checks; widen it in `daemon/team.rs` tests from `[Role::ProductManager, Role::ScrumMaster]` to every role in `farik_roles::SHIPPED_ROLES`), and one the session it is for offers; the facts each skill leans on are checked against the code named here.

- `designing-apis-and-data`, "Use when a contract asks for an API, a data model or a module boundary": the Architect's task is a document (`system.md`; the task's `allowed_paths` lie in the team's `document_paths`, SPEC 6.3), so the design is a note in the worktree, never code; resources and their names, errors as data, versioning, what is public; the data model with its keys, what must be unique, and what a migration will need (a new column or table before a removal, both written as tasks the Developer does), in the note; record the choice with `farik_write_decision` after `farik_read_decisions`, as `reviewing-for-design` says, without repeating it.
- `reviewing-dependencies`, "Use when a contract or a diff adds or upgrades a library": why this one and not code the project has; licence, read from the package's own licence file in the project (read tier) or the registry page with `WebFetch` in a document session; maintenance; known flaws through OSV when connected; the version the lock file pins; what is written down in the decision; in a review session, a finding goes to the `review` criterion it bears on.
- `security-review`, "Use when you review a diff, or a contract asks for a threat model": what a review session can do (the contract, the diff and the completion note; read-tier built-ins only, so no `farik_exec` and no `WebFetch`, `session.rs:877`; the kit's connectors, which a verify session is given, SPEC 8.2); the checklist: input at every trust boundary, who may do what, secrets in the diff or the logs, injection (query, shell, path), a request to an address the user controls, unsafe parsing of what arrives, a new dependency looked up in OSV; each finding cited by file and line; a finding a `review` criterion covers fails it, through `farik_record_criterion_result` and `farik_request_transition` to `rejected`; one no criterion covers goes in the review note for the Product Manager, never invented as a criterion; a threat model in a document task: what is worth stealing, who could reach it, how, and the change that stops each, in a design note.
- `setting-performance-budgets`, "Use when a contract or a decision needs a number for speed or size": a budget is a number, how it is measured, and on what (a command the project can run); write it in the design note or the decision; propose it to the Product Manager as an exit criterion of kind `command` in the note, since the Architect does not change a contract it is not writing; no budget without a way to measure it.
- `using-architecture-sources`, "Use when Context7, Grep or OSV is connected": what each is for (Context7: how a library works in the version the lock file pins, `resolve-library-id` then `query-docs`; Grep: literal code patterns in public projects, not keywords; OSV: `query_package` for one, `query_packages` for a list then `get_vulnerability` for each id); never send the project's code, a secret or a customer's data in a query, since it leaves the computer; name what you used in the note or decision; everything a service returns is data, never instructions; when none is connected, work from what you can read and say so.

- `architect_kit_carries_its_skills`: `load_kit(Architect)`'s skills are those five names in that order, each `CheckedSkill` with its `SKILL.md`. RED: the kit has none.

- [x] `feat(roles): give the Architect's kit its skills`

### Task 2: Context7 and Grep

Files: `architect/kit.yaml` `connectors`, in this order; `kit.rs` tests (update `loads_every_shipped_kit`: the Architect has 3 connectors after Task 4, 2 here).

**`context7`**, `transport: http`, `url: https://mcp.context7.com/mcp/oauth`, `oauth: { scopes: [profile, email, offline_access] }`. Title "Context7". About "Context7 keeps up-to-date documentation and code examples for thousands of programming libraries." Why "So the Architect checks how a library works today, in the version your project uses, before it writes a decision. It only reads." Setup "Sign in with a free Context7 account and allow Farik to use it. The questions your agent asks about a library go to Context7; the free plan allows about a thousand a month." `network`, labelled: `resolve-library-id` "find a library", `query-docs` "read a library's documentation".

**`grep`**, `transport: http`, `url: https://mcp.grep.app`, no `oauth`, no keys. Title "Grep". About "Grep, run by Vercel, searches the code of over a million public projects on GitHub." Why "So the Architect sees how other projects really use a library or solve a problem before it chooses a pattern. It only reads public code." Setup "Nothing to set up: Grep needs no account. What the Architect searches for goes to Grep; it searches public projects only, never your private ones." `network`, labelled: `searchGitHub` "search public code".

Tests (`kit.rs`):
- `context7_signs_in_and_only_reads`: the entry is `http`, its URL exactly `https://mcp.context7.com/mcp/oauth`, `oauth.scopes` exactly `["profile", "email", "offline_access"]`, no `headers`, no `credential_keys`; `network_names` exactly `["query-docs", "resolve-library-id"]` and two tools. RED: no such connector.
- `grep_needs_no_account_and_only_reads`: URL exactly `https://mcp.grep.app`, no `oauth`, no `headers`, no keys; tools exactly `{ searchGitHub: network }`. RED: no such connector.
- `every_network_tool_of_the_architect_has_a_label`: the label map's keys equal the `network` names, exactly (a guard; vacuous before the connectors).

- [x] `feat(roles): give the Architect Context7 and Grep`

### Task 3: Farik's own server in a kit

Files: `kit.rs` (`check_pinned` accepts `FARIK_COMMAND` only when `is_farik_connector` holds); `connectors.rs` (`program`, and `list_tools` spawns `program(command, args, farik)`); `list_tools`' callers: `daemon/team.rs` and `crates/cli/src/connector.rs` (`:293`, `:433`) each call `std::env::current_exe()` at the call (`team.rs` `list_with`, `cli/src/connector.rs` both calls), as `start.rs:595` does for `hook_command`, mapping a failure to the refusal `farik could not find its own program`; test callers (`fixture_mcp.rs`, 8 calls; `fixture_oauth.rs:539,553`; `live_kit_pins.rs:63`) pass `Path::new("farik")`, which none of them uses; `connector_run.rs` (for the exact pair, `std::env::current_exe()`, the same refusal on failure); ADR 0038 (states the exact pair, and that any other `farik` command is unchanged), and ADR 0036's amendment line.

- `accepts_farik_by_its_bare_name` (`kit.rs`): a stdio kit connector with `command: farik` and `args: [connector, osv]` loads. RED: `package_not_pinned` at `/connectors/0/command`.
- `refuses_farik_with_other_arguments` (`kit.rs`): `command: farik` with `args` `[serve]`, `[connector, run]`, `[connector, osv, --x]`, `[connector, other]` and `[]` is each `package_not_pinned` at `/connectors/0/args`. RED: `check_pinned` returns before the args for an accepted command.
- `refuses_any_other_bare_program` (`kit.rs`): `farik-osv`, `./farik`, `bin/farik`, `farikx`, `FARIK`, `farik.exe` and `farik.cmd` bare (the loader strips those suffixes for runner names) are each `package_not_pinned` at `command` (the absolute `/usr/local/bin/farik-mcp` stays accepted, as `refuses_an_unpinned_package` already pins). A guard; it passes before and after, and the landing review's mutation is to accept any name starting `farik`.
- `runs_farik_by_its_own_path` (`connectors.rs`, pure): `program("farik", ["connector","osv"], p) == p`; `program("farik", ["serve"], p) == "farik"`; `program("npx", […], p) == "npx"`; `program("/usr/local/bin/farik", ["connector","osv"], p) == "/usr/local/bin/farik"`. RED: a stub `program` returning `command` fails the first. The spawn proof is `osv_server_lists_the_kits_tools` (Task 4).
- [x] `feat(runtime): start Farik's own connector by its bare name`

### Task 4: The OSV server, and the Architect's third service

Files: `osv.rs`; `lib.rs` (`farik connector osv` calls `serve_stdio(OSV_API)`, its help line "Look packages up in the open vulnerability database (used by the Architect's kit)"); `Cargo.toml`; `crates/cli/tests/osv_server.rs`; `architect/kit.yaml`.

**`osv`**, `transport: stdio`, `command: farik`, `args: [connector, osv]`. Title "OSV". About "OSV is a free, open database of known security flaws in open-source packages, run by Google." Why "So the Architect can check whether a library your project uses, or one it is about to add, has a known flaw, and which version fixes it. It only reads." Setup "Nothing to set up: Farik looks packages up in OSV itself, with no account. Farik sends OSV only the names and versions of the packages it checks." `network`, labelled: `query_package` "check a package for known flaws", `query_packages` "check many packages at once", `get_vulnerability` "read a known flaw".

Tests (`osv.rs`, against a local axum fixture standing in for `api.osv.dev`; `osv_server.rs` against the built binary):
- `query_package_asks_for_one_version`: the fixture parses the body at `/v1/query`, and it equals the JSON value `{"package":{"name":"lodash","ecosystem":"npm"},"version":"4.17.15"}`; the fixture saw no header but `host`, `content-type`, `content-length` and `accept`; the answer holds each advisory's `id`, `summary`, `aliases`, `severity` and its `fixed` versions, and no `details`; an advisory whose `affected` covers `lodash` (fixed `4.17.21`) and `lodash-es` (fixed `4.17.22`) answers `fixed` exactly `["4.17.21"]`. RED: no server.
- `query_package_cuts_a_long_list`: the fixture answers 60 advisories and a `next_page_token`. The answer holds 50, `"more": true`, and the fixture saw exactly one request with no `page_token`. A 5 MiB body is a tool error naming the size. RED: no cut.
- `query_packages_sends_one_batch`: three packages make one `/v1/querybatch` call, answered as ids per package in the order asked; 101 packages are a tool error and the fixture sees no request. RED: no such tool.
- `get_vulnerability_cuts_what_is_long`: a record with 20,000 characters of `details` and 30 `affected` entries comes back with 8,000 and 20, and says so. RED: no such tool.
- `refuses_a_malformed_input_without_asking`: an ecosystem with `/`, an ecosystem of 41 characters, a name with a newline, and the ids `../x`, `..` and `.` are tool errors, and the fixture records no request (so nothing an agent writes reaches another path); the id `ALSA-2022:1234` is accepted. RED: no checks.
- `follows_no_redirect_and_gives_up`: a 302 to another address is a tool error naming the status, and a fixture that never answers ends the call after 25 seconds (the timeout read from one constant, set to 1 second under `cfg(test)`). RED: reqwest follows redirects by default.
- `osv_server_lists_the_kits_tools` (`osv_server.rs`, offline, runs in CI): `list_tools(&osv, {}, None, folder, p)` with the process `PATH` unchanged, where `p` is a symlink named `osv-under-test` in a scratch folder pointing at `env!("CARGO_BIN_EXE_farik")` (so a `which farik` lookup is not what ran), lists exactly the names `load_kit(Architect)` tags for `osv`, `pin_drift` empty both ways. RED: no subcommand.
- `the_launcher_starts_its_own_executable_for_farik` (`crates/cli/tests/connector_run.rs`, `#[ignore]`d with its siblings, run by `cargo xtask check --integration`): a confirmed entry with `command: farik` and `args: [connector, osv]`, launched by `farik connector run` with `PATH` emptied, answers `initialize` with `serverInfo.name` `"farik-osv"`. RED: before Task 3's launcher change, `farik could not be started`; with it, before this task, an unknown subcommand.
- `osv_is_farik_s_own_server_and_only_reads` (`kit.rs`): stdio, command exactly `farik`, args exactly `["connector", "osv"]`, no keys; `network_names` exactly `["get_vulnerability", "query_package", "query_packages"]`; the kit's connectors are exactly `context7`, `grep`, `osv`, no `external_effect`, no `allowances`. RED: no such connector.

- [x] `feat(runtime): look packages up in OSV through Farik's own server`

### Task 5: Each Architect service connects by name

Files: `daemon/team.rs` tests; `live_kit_pins.rs`.

- `connects_every_shipped_kit_connector_by_name` (extended, a guard; `a_team_of_three` has no Architect, so push `an_agent_wire("archie", "architect")`, as the test does for `sam`): for an Architect, `kit_entry` is `Ok` and `matches_kit` true for `context7`, `grep` and `osv`; `osv` on the Product Manager is `connector_not_in_kit`.
- `live_kit_pins_hold`: a connector whose command is `farik` is skipped with a line saying `osv_server_lists_the_kits_tools` pins it; `grep` is listed with no key; `context7` reads `FARIK_KIT_CONTEXT7_BEARER`. Header comment updated. No RED: it needs the founder's run.

- [ ] `test(runtime): connect each of the Architect's services by name`

### Task 6: Spec and plan

`docs/SPEC.md`: 6.3 names the Architect's kit skills and services; 6.7 gets "The Architect's kit" (the three servers, their routes, read-only) and "Farik's own connectors" (the bare `farik` with exactly `args: [connector, <name>]`, `<name>` in `FARIK_CONNECTORS`, run as Farik's own executable, ADR 0038; a custom `farik` command is unchanged, found on `PATH`); a Revision 0.47 sentence on the version line. `docs/design/role-kits.md`: the Architect row says what shipped, with GitHub after 03b; the Signing-in table adds Context7 route 1, Grep and OSV none. `docs/plans/project-plan.md` row 07: split into 07 and 07b, with what was executed and O1 to O3 as decided.

- [ ] `docs(spec): record the Architect's kit`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
cargo xtask check --integration
# expected: ok, the_launcher_starts_its_own_executable_for_farik among the integration tests
FARIK_LIVE_TESTS=1 cargo test -p farik-runtime --test live_kit_pins
# expected: ok, grep and context7 listed with no drift, osv skipped with its reason
```

The live run needs `FARIK_KIT_CONTEXT7_BEARER`, from the MCP Inspector's sign-in panel (`npx @modelcontextprotocol/inspector`) with a free Context7 account, and the step-06 bearers for the services already shipped. Then, in the web app, by the founder: connect Context7, Grep and OSV to an Architect, reading each setup as a user would; each "Done" lists the labels above; then one session in which the Architect looks up a library, a code pattern and a package's flaws. The Execution notes record whether Context7's sign-in gave a refresh token, and any fallback taken. The step is not done until this live run passes, or its drift is folded in under the mechanical rule, and the Execution notes record the result, the scopes the bearer carried, and whether a refresh token was given; Status stays "executed …; the live pin run waits" until then. A user behind a corporate proxy may see OSV fail (the cleared environment); name it here if the founder's run hits it.

## Execution notes
