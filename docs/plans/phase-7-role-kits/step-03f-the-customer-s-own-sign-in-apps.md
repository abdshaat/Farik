# Phase 7, step 03f: The customer's own sign-in apps

Status: draft. Its readiness review runs once Task 1's boards are approved (ADR 0032: one round).
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 6.7, 8.4, 8.6, 9; F9
Depends on: step 03b of this phase (Tasks 1 to 6, landing-reviewed: `RegisteredApp`, `AppFlow`, `app_for`, the device flow, `OAuthGrant.app`, `set_registered_apps`, `CliIo.registered_apps`, `provider` and `settings_url` on the wire, `CodeCard`); step 08e (executed; its landing review's fixes committed before Task 3 starts, which at the time of writing make `GOOGLE_CLIENT_ID` an `Option` and ship no Google entry for an empty secret); step 05 (`KitConnect`); phase 6 (Settings, merged in #19). Numbered with the sign-in steps 03 to 03e and run after 08e, because it rewrites 03b's and 08e's table; 08f depends on it. Before Task 2 the executor re-reads every file:line here against HEAD and records corrections in Execution notes (not a second review).
Readiness confirmed by: not yet run
Mockups approved by: pending (Task 1's gate)
Decided by the founder, 2026-10-06, in conversation: O1, publish ("Publish it"); O2, the fixed page ("Fixed GitHub page").

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

ADR 0043: until phase 15, every login is the customer's. After this step, a customer gives Farik their own GitHub App's client id, and their own Google Desktop client's id and secret, each with a how-to: in the web app, on a "Sign-in apps" section of Settings and wherever a sign-in finds no app of theirs for its provider (`ConnectorAdd`, `KitConnect`), and with `farik sign-in-app` on the command line. Farik checks their shape, keeps the ids in the machine's settings and the secret in the keychain, signs in with them through 03b's and 08e's flows, refuses one on another provider's address, and lapses a grant made with an app the customer removes or replaces. The build-time secret and Farik's placeholder id leave the code: the table keeps only the providers' fixed facts. Out of scope: Google Ads itself (08f); Farik's own apps, offered beside the customer's in phase 15; any other provider.

## Decisions

- **Two tables, one fixed and one the customer's.** `Provider` holds a provider's fixed facts in code, as `RegisteredApp` does today less the client: id, name, host or Farik connector, flow, scopes, issuer, token and revocation endpoints, install and settings pages, whether it needs a secret, and its how-to. `RegisteredApp` becomes the customer's app: `{ provider: &'static Provider, client_id: String, client_secret: Option<Secret> }`, built only by `check_app`, its `Debug` redacted through `Secret`'s. `REGISTERED_APPS`, `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`, `google()` and the `option_env!` go, with the `the_shipped_table_*` tests. Rejected: leaking runtime strings into the `&'static` table (one leak per save, for the daemon's life).
- **`PROVIDERS`**: `github` (03b's entry: host `api.githubcopilot.com`, Device flow, no scopes, no revocation, settings `https://github.com/settings/apps/authorizations`, no secret) and `google` (08e's entry: no host, `farik_connector: Some("google-ads")`, Loopback, the one scope `https://www.googleapis.com/auth/adwords`, no revocation, settings `https://myaccount.google.com/connections`, a secret). GitHub's `install_url` becomes `https://github.com/settings/installations` (O2).
- **Where they are kept** (`farik_runtime::sign_in_apps`). Client ids in `sign_in_apps.json` in the user's Farik state folder (8.4), `{ "apps": { "<provider>": { "client_id": "…" } } }`, the folder 0700 and the file 0600, written beside and renamed over while `sign_in_apps.json.lock` holds off another process, as `connectors.json` is (ADR 0030). A secret in the OS keychain, service `farik`, account `sign_in_app:<provider>`, through the `OpenEntry` seam `KeychainConnectorSecrets` uses; on a computer with no keychain, in `sign_in_secrets.json` beside it, 0600, `{ "<provider>": "<secret>" }`, as ADR 0022 and ADR 0030 fall back. With no state folder nothing is kept and saving is refused `sign_in_apps_no_folder` (templates' `no_state_folder` carries their own words on the page). An entry that fails `check_app` when read, or names no provider, is not an app. Rejected: refusing a computer without a keychain (ADR 0022's precedent keeps such a computer working).
- **Never on the wire.** No answer, event, log or error carries a client id or a secret; `sign_in_apps.list` says only whether each provider is set up. The one exception OAuth forces: the authorization address of a loopback sign-in (Google's) carries `client_id`, and `connector.sign_in` answers it as `authorize_url`. Saving or removing records no event: the setting is the machine's, not the project's, as templates are (spec 0.35).
- **Fresh on each use.** The daemon reads the store at each `begin_sign_in` and each refresh, so an app saved by `farik sign-in-app` while `farik serve` runs is used at once; `read_kept` reads the ids file alone (`client_ids`), never the keychain, as `team.get` must not ask one each time.
- **A removed or replaced app lapses its grants.** `refreshed` finds the grant's app by `grant.app` equal to the provider's id and `client_id` equal to `grant.client_id`; with none it answers `Lapsed` before the due check, with no request, so 08e's rule for an app the table lacks holds as written. `read_kept` sets `SignedIn.app_changed`, and `lapsed` with it, when a grant's `app` has no kept client id or another one, so the server is left out as a lapsed one is; `team.get`'s row says `sign_in_again` with `app_changed: true`.
- **No app yet is its own refusal.** On an address a provider serves (`provider_for`), with no customer app for it and no `oauth.client_id` in the team file, `start_sign_in` answers `SignInError::AppMissing(<provider id>)` before any request; so does the pair `farik connector <name>` that a provider signs in for (`provider_for_farik_connector`) with no app. The daemon answers it `sign_in_app_missing`, its refusal item `{ path: "/server", code, message, provider }`. 03b's rules stand otherwise: a team file's `client_id` equal to a customer app's is refused on any other provider's address (`Failed("this sign-in is only for <name>'s own servers")`), used on its own; any other `client_id` runs step 03.
- **Shape checks**, hand-written (the runtime has no `regex`), after trimming spaces and line ends, each at most 200 characters (facts the readiness review verifies, below):
  - GitHub: 20 characters, `Iv` then ASCII letters, digits or `.`; one starting `Ov` is an OAuth app's ("That is an OAuth app's client ID. Farik needs a GitHub App's, which starts with Iv."); any secret given is refused ("Your GitHub app needs no client secret. Leave it empty.").
  - Google: the id is digits, `-`, then 1 to 64 lower-case letters or digits, then `.apps.googleusercontent.com`; the secret is required and is `GOCSPX-` then 20 to 64 ASCII letters, digits, `-` or `_`.
  - Codes: `sign_in_app_unknown`, `sign_in_app_client_id`, `sign_in_app_secret`, each at its field.
- **The how-to is data**, `how_to: &'static [HowToStep]`, so the web app and `farik sign-in-app how-to` say the same words, the boards' (Task 1), exactly as below; a step's link has its own words, then its address. Each step stands alone, with no "above" or "below", because the Settings card, the two dialogs and the command line all show it; GitHub's and Google's own labels are quoted exactly as they show them. Google's step 5 is O1's; GitHub's step 10 comes before O2's link.
  - GitHub's:
    1. On GitHub, signed in to your own account, start a new GitHub App. Link: "Open GitHub’s new app page", `https://github.com/settings/apps/new`.
    2. “GitHub App name”: a name no one else on GitHub uses, such as “Farik for” and your business’s name.
    3. “Homepage URL”: any address, such as your business’s website.
    4. Leave “Callback URL” empty, and “Request user authorization (OAuth) during installation” off.
    5. Turn on “Enable Device Flow”, and leave “Expire user authorization tokens” on.
    6. Under “Webhook”, turn off “Active”.
    7. Under “Repository permissions”, set “Contents”, “Issues” and “Pull requests” to “Read-only”. “Metadata” is already “Read-only”. Change nothing else.
    8. Under “Where can this GitHub App be installed?”, choose “Only on this account”, then “Create GitHub App”.
    9. On the page GitHub shows next, copy the “Client ID”. Make no client secret: Farik needs none.
    10. Choose “Install App”, then “Install”, and pick the repositories your agents may read.
  - Google's:
    1. On Google Cloud, signed in to the Google account that manages your ads, create a project. Any name will do. Link: "Open Google Cloud’s new project page", `https://console.cloud.google.com/projectcreate`.
    2. In that project, turn on the Google Ads API: choose “Enable”. Link: "Open the Google Ads API’s page", `https://console.cloud.google.com/apis/library/googleads.googleapis.com`.
    3. Open Google Auth Platform and choose “Get started”. Give your app a name and your email. For its audience, choose “External”, or “Internal” if your business uses Google Workspace. Link: "Open Google Auth Platform", `https://console.cloud.google.com/auth/overview`.
    4. Under “Data Access”, add the scope https://www.googleapis.com/auth/adwords, and save.
    5. If you chose “External”: under “Audience”, choose “Publish app”, so you need not sign in again every 7 days. When you sign in, Google warns that it has not verified your app. The app is yours, so go on.
    6. Under “Clients”, choose “Create client”, with “Desktop app” as its type and any name. Copy its “Client ID” and its “Client secret”: Google shows the secret only once.
    7. On your project’s Google Ads API page, apply for Explorer access. A new project starts with Test access, which reaches test accounts only.
- **Facts the readiness review checks** against GitHub's and Google's pages of its day, changing the plan where they differ: the two GitHub client id formats; Google's id suffix and the `GOCSPX-` prefix; the GitHub App settings and labels the how-to quotes, and `https://github.com/settings/installations` with its “Configure”; that a published unverified app's refresh tokens do not end after 7 days, the unverified-app screen it shows, and its user cap; the Internal audience; the Google Auth Platform labels the how-to quotes (“Get started”, “Data Access”, “Audience”, “Publish app”, “Clients”, “Create client”, “Desktop app”) and that a new client's secret is shown only once; the Google Ads API's access levels for a new project.
- **`farik_connector_client` keeps refusing** `client_id` and `callback_port` on the pair, since the customer's app lives on the computer, not in the team file (ADR 0043); its sentence becomes "farik_connector_client: Farik's own connector signs in with your own app for its service, which you give Farik in Settings, not in the team file".
- **Screens**, in the words of Task 1's boards (`SignInApps.dc.html`, `PhoneSignInApps.dc.html`):
  - A "Sign-in apps" section on Settings after "Saved teams": "Farik has no app of its own at GitHub or Google, so it signs your agents in with apps you make there, once for every project on this computer."; a row per provider ("Signs your agents in to GitHub." or "… to Google Ads.", "Not set up" with "Set up", or "Set up" with "Change" and "Remove"); under them, where they are kept.
  - One card, `SignInAppCard`: the provider's how-to, each link opening a new tab; "Client ID" ("Starts with Iv." or "Ends with .apps.googleusercontent.com."); for Google, "Client secret", a password field never filled in ("Starts with GOCSPX-. Farik keeps it on this computer and never shows it again."); a refusal under its field. Settings shows it in the row's place, titled "Set up your GitHub app" or "Change your Google app", Change adding "Saving replaces the app Farik has now. Agents signed in with it will need to sign in again.", with Save and Cancel.
  - `ConnectorAdd` and `KitConnect`, on `sign_in_app_missing`, show the card in the sign-in button's place, led by "GitHub lets Farik sign in only with a GitHub app of your own. Make it once, and every agent on this computer can sign in with it." (Google's alike), with "Save and sign in", which saves and then asks `connector.sign_in` again: GitHub's answer shows 03b's code; for Google the same click opens the sign-in tab, as `KitConnect`'s sign-in does (a tab opened in the click, its `opener` cleared, sent to the answer's `authorize_url`, closed on a refusal). `ConnectorAdd` keeps "Use a key instead".
  - Remove asks in the row, as Saved teams does: "Remove your Google app? Agents signed in with it will need to sign in again." and "Farik forgets it on this computer. The app stays in your Google Cloud project until you delete it there.", with "Remove it" and "Keep it".
  - `AgentEdit`'s row for `app_changed`: "You changed your GitHub app. Sign in again to use it." with "Sign in again".
  - 03b's private-repository line (O2): "To let {name} read private repositories, choose them for your app on {provider}." with the link "Choose repositories on {provider}" to `install_url`, in place of `addInstallLine` and `addInstallLink`.

Answered by the founder on 2026-10-06, both as recommended:
- **O1, Google's publishing status.** The how-to tells the customer to publish their app for their own use, so their sign-in does not end every 7 days; they then see Google's unverified-app warning on their own consent screen. The alternative, leaving it in Testing with themselves as test user, means signing in again weekly. Recommendation: publish.
- **O2, GitHub's private repositories.** After signing in, the line for private repositories links to `https://github.com/settings/installations`, where the customer chooses what their app may read, and the how-to installs the app first. The alternative asks for the app's public link as a second field to build `…/apps/<slug>/installations/new`. Recommendation: the fixed page, one field fewer.

## File map

```
docs/design/mockups/SignInApps.dc.html, PhoneSignInApps.dc.html, canvas.json     Task 1
crates/core/src/team.rs, docs/schemas/team.schema.json                          modifies: farik_connector_client's words (Task 2)
crates/runtime/src/registered_apps.rs                                            modifies: Provider, PROVIDERS, HowToStep, HowToLink, RegisteredApp, check_app, the lookups (Task 3)
crates/runtime/src/{sign_in.rs,daemon.rs,daemon/signed_in.rs,daemon/team.rs,orchestrator/session.rs}, crates/runtime/tests/fixture_oauth.rs, crates/cli/src/{lib.rs,connector.rs}, crates/cli/tests/connector.rs   modifies: what Task 3's types force; AppMissing and the refresh (Task 4)
crates/runtime/src/sign_in_apps.rs, lib.rs                                        creates: the store (Task 5)
crates/runtime/src/{daemon.rs,daemon/signed_in.rs,daemon/team.rs}, crates/cli/src/{lib.rs,start.rs,connector.rs}   modifies: the store in place of set_registered_apps (Task 5)
crates/runtime/src/daemon/sign_in_apps.rs, daemon/web.rs, docs/schemas/rpc.schema.json   creates and modifies: the three RPCs, sign_in_app_missing (Task 6)
crates/cli/src/{lib.rs,sign_in_app.rs,connector.rs}, crates/cli/tests/sign_in_app.rs      creates and modifies: farik sign-in-app (Task 7)
apps/web/src/pages/{SignInAppCard.tsx,SignInApps.tsx,Settings.tsx,ConnectorAdd.tsx,KitConnect.tsx,AgentEdit.tsx,signInApps.test.tsx,connectors.test.tsx}, apps/web/src/app/refusals.ts, apps/web/src/strings/en.ts   (Task 8; and registered_apps.rs's how-to words, if the approval changed them)
docs/SPEC.md, docs/design/role-kits.md, docs/plans/project-plan.md, docs/plans/phase-7-role-kits/step-03b-farik-s-registered-apps.md   modifies (Task 9)
```

## Interfaces

Consumes: `AppFlow`, `RegisteredApp`, `app_for`, `app_for_farik_connector`, `start_sign_in`, `start_app_sign_in`, `refreshed`, `SignInError`, `OAuthGrant` (with `app`, `client_id`), `begin_sign_in`, `read_kept`, `SignedIn`, `set_registered_apps`, `CliIo.registered_apps`, `SignInWith` (03b, 08e); `Secret` (`claude.rs`); `CredentialError`, `map_keyring_error`, `read_keychain` (`credential.rs`); `OpenEntry`, `SERVICE` (`connectors.rs`, `OpenEntry` made `pub(crate)`); `state_dir` (`cli/src/state.rs`); `CodeCard`, `refusalsOf` (web).

Produces:

```rust
// farik-runtime, registered_apps.rs (Task 3)
pub struct HowToLink { pub text: &'static str, pub url: &'static str }      // the link's own words, and its address
pub struct HowToStep { pub text: &'static str, pub link: Option<HowToLink> }
pub struct Provider { pub id: &'static str, pub name: &'static str, pub host: Option<&'static str>,
    pub farik_connector: Option<&'static str>, pub flow: AppFlow, pub scopes: &'static [&'static str],
    pub issuer: &'static str, pub token_endpoint: &'static str, pub revocation_endpoint: Option<&'static str>,
    pub install_url: Option<&'static str>, pub settings_url: &'static str, pub needs_secret: bool,
    pub how_to: &'static [HowToStep] }                                   // Debug, Clone, Copy, PartialEq, Eq
pub static PROVIDERS: &[Provider];
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredApp { pub provider: &'static Provider, pub client_id: String, pub client_secret: Option<Secret> }
pub enum AppRefusal { ClientId(String), Secret(String) }                // the sentence; Display
pub fn check_app(provider: &'static Provider, client_id: &str, client_secret: Option<&str>) -> Result<RegisteredApp, AppRefusal>;
pub fn provider_for<'a>(providers: &'a [Provider], url: &str) -> Option<&'a Provider>;
pub fn provider_for_farik_connector<'a>(providers: &'a [Provider], command: &str, args: &[String]) -> Option<&'a Provider>;
pub fn app_for<'a>(apps: &'a [RegisteredApp], url: &str) -> Option<&'a RegisteredApp>;          // by app.provider.host
pub fn app_for_farik_connector<'a>(apps: &'a [RegisteredApp], command: &str, args: &[String]) -> Option<&'a RegisteredApp>;
// farik-runtime, sign_in.rs (Task 4)
pub enum SignInError { /* as before */ AppMissing(String) }
pub async fn start_sign_in(url: &str, settings: &OAuthSettings, providers: &[Provider], apps: &[RegisteredApp],
    now: DateTime<Utc>) -> Result<SignIn, SignInError>;
// farik-runtime, sign_in_apps.rs (Task 5)
pub struct KeptApp { pub provider: String, pub client_id: String, pub client_secret: Option<Secret> }   // Debug, Clone
pub trait SignInAppStore: Send + Sync {
    fn load(&self) -> Result<Vec<KeptApp>, CredentialError>;
    fn client_ids(&self) -> Result<BTreeMap<String, String>, CredentialError>;   // the ids file alone
    fn save(&self, app: &KeptApp) -> Result<(), CredentialError>;               // replaces the provider's
    fn remove(&self, provider: &str) -> Result<(), CredentialError>; }          // nothing kept is no error
pub struct StateFolderApps;  impl StateFolderApps { pub fn new(state: Option<PathBuf>) -> StateFolderApps; }
pub struct MemoryApps;       impl MemoryApps { pub fn holding(apps: Vec<KeptApp>) -> MemoryApps; }   // and Default; for tests
pub fn registered(providers: &'static [Provider], kept: Vec<KeptApp>) -> Vec<RegisteredApp>;  // check_app each; drops failures
// farik-runtime, daemon.rs (Task 5): in place of set_registered_apps and registered_apps()
impl DaemonState { pub fn set_providers(&self, providers: &'static [Provider]) -> bool;
    pub fn set_sign_in_apps(&self, apps: Arc<dyn SignInAppStore>) -> bool;
    pub(crate) fn providers(&self) -> &'static [Provider];                       // unset: PROVIDERS
    pub(crate) fn sign_in_apps(&self) -> Result<Vec<RegisteredApp>, CredentialError>; }   // read now
pub(crate) struct SignedIn { /* as before */ pub app_changed: bool }
// farik cli (Task 5): CliIo's `registered_apps` becomes `providers: &'static [Provider]` and
// `sign_in_apps: Arc<dyn SignInAppStore>` (MemoryApps in `new`, StateFolderApps in `main`)
```

Wire (`snake_case`): `sign_in_apps.list {} → { providers: [{ id, name, set, needs_secret, how_to: [{ text, link?: { text, url } }] }] }`; `sign_in_app.save { provider, client_id, client_secret? } → {}`; `sign_in_app.remove { provider } → {}`; `connector.sign_in`'s refusal item for `sign_in_app_missing` gains `provider`; `team.get`'s connector row gains `app_changed?` (true only).

## Tasks

### Task 1: The sign-in app screens, mocked up

An Opus agent draws these on the canvas (https://claude.ai/artifact/6tNaCmNojhixuiJBsDPsmf, the Connectors page), each at desktop and phone width, in the canvas's tokens, and copies them to `docs/design/mockups/`:

- **Settings, "Sign-in apps"**: GitHub "Not set up" with "Set up"; Google "Set up" with "Change" and "Remove"; a line saying why: "Farik has no app of its own at GitHub or Google, so it signs your agents in with apps you make there, once for every project on this computer."
- **GitHub's card**: its how-to steps with "Open GitHub's new app page", the "Client ID" field, Save; and the refusal under the field for an OAuth app's id.
- **Google's card**: its how-to steps with their links, "Client ID", "Client secret" (a password field), Save.
- **`ConnectorAdd`** for `https://api.githubcopilot.com/mcp/` with no GitHub app: GitHub's card in place of the sign-in button, "Save and sign in", and "Use a key instead".
- **`KitConnect`**, Google Ads for the Marketing Specialist with no Google app: Google's card, "Save and sign in".
- **Remove's confirmation**: "Remove your Google app? Agents signed in with it will need to sign in again."
- **`AgentEdit`**, a row whose app changed: "You changed your GitHub app. Sign in again to use it." with "Sign in again"; and 03b's signed-in board with the private-repository line of O2.

Gate: the founder approves the boards (O1 and O2 are answered, above), or says to approve them automatically; the approval and the answers are written into this plan's header with the date. Task 8 does not start until then. Tasks 2 to 7 do not wait for it: Task 3 writes the how-to in the boards' words as drawn, with O1 and O2 as recommended; a word the founder changes in approving, and a fact a reversal of O1 or O2 changes (Google's "Publish app" step, GitHub's `install_url` and the test that asserts it), are changed in `registered_apps.rs` in Task 8's commit, the one file two tasks touch.

- [ ] `docs(design): mock up the customer's own sign-in apps`

### Task 2: The team file's words

Files: `crates/core/src/team.rs`, `docs/schemas/team.schema.json` (the `oauth` description: the pair signs in with the customer's app for its service).

- `the_farik_connector_takes_no_client_of_its_own` (changed): each refusal's message is exactly Decisions' new sentence. RED.

- [ ] `feat(core): say Farik's own connector signs in with the customer's app`

### Task 3: The providers and the customer's app

Files: `registered_apps.rs`, and what the new types force elsewhere (File map), each change mechanical: `app.<fact>` becomes `app.provider.<fact>`, `app.client_id` a `&str` of the `String`, the secret `client_secret.as_ref().map(Secret::expose)`; `Setup.app` stays `Option<RegisteredApp>` (now `Clone`, not `Copy`); every test table becomes a leaked `Provider` and a `RegisteredApp` built by `check_app` or a literal; `DaemonState.registered_apps` and `CliIo.registered_apps` keep their type and default to `&[]` until Task 5, which is what ships today.

- `the_providers_table_holds_github_and_google`: `PROVIDERS` is exactly `github` then `google`, each fact as 03b's and 08e's entries have it, with Decisions' changes (GitHub's `install_url`, `needs_secret`); every address `https`; each has a non-empty `how_to` whose first step has a link. RED.
- `github_s_how_to_names_every_setting` and `google_s_how_to_names_every_setting`: the steps' text, joined, names each setting and link Decisions list for it. RED each.
- `checks_a_github_client_id`: `Iv23liAbCdEfGh123456` and `Iv1.0123456789abcdef` pass, the latter with spaces and a line end around it, trimmed; an `Ov…` id gives the OAuth-app sentence; empty, 19 and 21 characters, a space inside and a non-ASCII letter give `ClientId`; any secret gives `Secret`. RED.
- `checks_a_google_client`: a well-shaped id (twelve digits, a dash, 32 lowercase letters and digits, `.apps.googleusercontent.com`) with a secret of `GOCSPX-` and 28 characters passes, both built at run time from their parts so that no literal in the repository matches GitHub's secret scanning, which refuses the push of a Google-shaped client id or secret even when it is fake; no suffix, a capital, no digits before `-`, a missing secret, a secret without the prefix, with a space, and of 72 characters after the prefix are each refused at their field. RED.
- `a_registered_app_does_not_print_its_secret` (08e's, changed): `format!("{app:?}")` of a Google app holds `[redacted]` and not the secret. Guard.
- 03b's `matches_github_by_its_exact_host`, `matches_a_loopback_fixture_over_http`, `an_entry_without_a_host_matches_no_address` and `matches_a_farik_connector_by_its_exact_pair` run over customer apps of test providers; `provider_for` and `provider_for_farik_connector` get the same cases. RED for the two new functions.

- [ ] `refactor(runtime): keep the providers' facts apart from the customer's app`

### Task 4: Signing in and refreshing with the customer's app

Files: `sign_in.rs`, `tests/fixture_oauth.rs`; `start_sign_in`'s callers pass `PROVIDERS` until Task 5.

- `a_provider_s_address_without_an_app_asks_for_one`: a Device test provider `dev` and no app: its address gives `AppMissing("dev")` and the fixture saw no request; with `oauth.client_id` set to another id, step 03's discovery runs. RED.
- `signs_in_with_the_customer_s_device_app`: `/device/code` and `/token` got the customer app's `client_id`; the grant's `client_id` is it and `app` is `dev`. Guard (03b's flow with the new type).
- `signs_in_with_the_customer_s_google_client`: the Loopback test provider's address carries the customer's id; `/token` got the customer's secret. Guard.
- `the_customer_s_client_id_is_refused_elsewhere` (03b's `farik_s_client_id_is_refused_elsewhere`, renamed): the customer's id in `oauth.client_id` at an address outside its provider gives `Failed` naming the provider, no request. Guard.
- `a_grant_of_a_removed_or_replaced_app_lapses`: a grant with `app: Some("dev")` against no `dev` app, and against a `dev` app with another client id, gives `Lapsed` with no request, due or not. RED for the replaced case.
- `refreshes_with_the_customer_s_secret`: the refresh sends the current app's secret. Guard.

- [ ] `feat(runtime): sign in and refresh with the customer's own app`

### Task 5: Keeping the customer's apps

Files: `sign_in_apps.rs`, `lib.rs`, `connectors.rs` (`OpenEntry` `pub(crate)`), `daemon.rs`, `daemon/signed_in.rs`, `daemon/team.rs`, `orchestrator/session.rs`, `cli/src/{lib.rs,start.rs,connector.rs}`. `set_registered_apps` and `CliIo.registered_apps` go; every harness sets `set_providers` with its test providers and `set_sign_in_apps` with a `MemoryApps` holding its client. `begin_sign_in` and `refresh_under_lock` read `sign_in_apps()` (a store error refuses the sign-in `sign_in_failed: <the store's words>` and leaves a refresh as a failed one); `farik serve` hands the daemon `CliIo`'s store.

- `keeps_ids_in_the_state_folder_and_secrets_in_the_keychain`: saving Google's app writes the id and not the secret to `sign_in_apps.json` (0600, folder 0700) and the secret to the mock keychain at `farik`/`sign_in_app:google`; `load` answers both. RED.
- `keeps_the_secret_in_a_private_file_without_a_keychain`: with a keychain answering `NoKeychain`, `sign_in_secrets.json` (0600) holds it and `load` finds it. RED.
- `remove_forgets_the_id_and_the_secret` and `saving_again_replaces_the_app`. RED each.
- `client_ids_never_asks_the_keychain`: with a keychain failing every call, `client_ids` answers the file's ids. RED.
- `an_app_that_fails_its_checks_is_not_an_app`: a file with a malformed id, and one naming an unknown provider, gives no `RegisteredApp` from `registered`. RED.
- `no_state_folder_keeps_nothing`: `StateFolderApps::new(None)` loads nothing and refuses `save` with `sign_in_apps_no_folder`. RED.
- `a_sign_in_uses_the_app_saved_since_the_daemon_started` (`daemon/team.rs`): an app saved to the store after the harness starts is the one `connector.sign_in` uses. RED.
- `team_get_says_the_app_changed` (`daemon/team.rs`): a kept `dev` grant whose client id the store no longer has gives the row `sign_in_again` and `app_changed: true`, and a session leaves it out; with the same id the row has no `app_changed`. RED.

- [ ] `feat(runtime): keep the customer's sign-in apps on the computer`

### Task 6: Setting up an app from the web app

Files: `daemon/sign_in_apps.rs` (`METHODS`, `QUERIES`, as `daemon/templates.rs`), `daemon/web.rs`, `daemon/signed_in.rs` (`refusal_of`; `begin_sign_in` answers `AppMissing` for the pair a provider signs in for with no app), `daemon/team.rs` (`connector_sign_in` puts `provider` in the refusal item), `rpc.schema.json` (the three calls, `app_changed`, and the descriptions at :926, :4580 and :4588 that say "Farik's own app").

- `lists_each_provider_and_whether_it_is_set`: the answer names `github` and `google` with `set`, `needs_secret` and the how-to, and its text holds neither a saved client id nor a saved secret. RED.
- `saves_checks_and_removes_an_app`: a good save answers `{}` and the store has it; a bad id, a missing secret, an unknown provider and no state folder are refused `sign_in_app_client_id` at `/client_id`, `sign_in_app_secret` at `/client_secret`, `sign_in_app_unknown` at `/provider` and `sign_in_apps_no_folder` at `/`; remove answers `{}` twice. RED.
- `a_sign_in_without_the_app_asks_for_it`: `connector.sign_in` for the `dev` address, and for `farik connector osv` under the Loopback test provider, with no app, is refused `sign_in_app_missing` with `provider` naming it. RED.
- `setting_up_an_app_records_nothing`: no event is appended by save or remove. Guard.

- [ ] `feat(runtime): set up the customer's sign-in apps from the web app`

### Task 7: `farik sign-in-app`

Files: `cli/src/lib.rs` (the subcommand), `cli/src/sign_in_app.rs`, `cli/src/connector.rs` (`start_signing` answers `AppMissing` for the pair as the daemon does; `refused` words it), `cli/tests/sign_in_app.rs` (through `run_with`, with `CliIo`'s `MemoryApps` and the test providers).

- `lists_and_explains`: `farik sign-in-app list` prints `github  GitHub  not set up` and `google  Google  not set up`; `farik sign-in-app how-to github` prints the steps numbered from 1, each link's words and address on the line after its step. RED.
- `sets_and_removes`: `set github --client-id <id>` prints `Saved your GitHub app.`; `set google --client-id <id>` reads the secret from one line of standard input and prints `Saved your Google app.`; a refused shape prints the sentence and exits 1; `remove google` prints `Removed your Google app. Agents signed in with it will need to sign in again.`; no output holds the secret. RED.
- `farik_connect_says_how_to_add_the_app`: `farik connect` to the `dev` address with no app exits 1 with `Farik signs in to Dev with an app of your own. Run "farik sign-in-app how-to dev", then "farik sign-in-app set dev --client-id <id>".` RED.

- [ ] `feat(cli): set up the customer's sign-in apps from the command line`

### Task 8: The screens

Files: `SignInAppCard.tsx`, `SignInApps.tsx`, `Settings.tsx`, `ConnectorAdd.tsx`, `KitConnect.tsx`, `AgentEdit.tsx`, `refusals.ts` (`Refusal` gains `provider?`), `strings/en.ts`, `signInApps.test.tsx`, `connectors.test.tsx`; `registered_apps.rs` for a how-to word or an O1 or O2 fact the approval changed (Task 1). Built from Task 1's boards.

- `settings_lists_the_sign_in_apps`: the section shows each provider's state; "Set up" opens its card; Remove asks, then calls `sign_in_app.remove`.
- `the_card_saves_and_shows_refusals`: Save sends `sign_in_app.save` with the trimmed fields (the secret field only where `needsSecret`); a refusal shows under its field; the secret field is `type="password"` and never prefilled.
- `connector_add_asks_for_the_app_then_signs_in`: on `sign_in_app_missing` the provider's card shows; "Save and sign in" saves, then calls `connector.sign_in` again and shows 03b's code card.
- `kit_connect_asks_for_the_app_then_signs_in`: the same on `KitConnect` for Google.
- `agent_edit_says_the_app_changed`: a row with `appChanged` says the boards' sentence and offers "Sign in again".
- axe on the section and on both cards.

- [ ] `feat(web): set up the customer's sign-in apps`

### Task 9: Spec, decisions and plan

`docs/SPEC.md`: 6.7's "Farik's own apps" (0.58) and "Signing in with Google" (0.59) paragraphs as built: the providers table, the customer's app, the store, `sign_in_app_missing`, `app_changed`, the screens and the command line; 8.4 the two state-folder files; 8.6 that no credential of Farik's ships and the client id and secret never reach the wire but in a loopback authorization address; F9 the three calls; revision 0.61. `docs/design/role-kits.md` (its Signing-in rows as built). The project plan's row 03f. Step 03b's live check, now the founder's as a customer.

- [ ] `docs(spec): record the customer's own sign-in apps`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check)
grep -rnE --exclude-dir=node_modules --exclude-dir=target 'option_env!|FARIK_GOOGLE_CLIENT_SECRET|GOOGLE_CLIENT_ID' crates apps .github
# expected: no output
```

The founder's live check, as a customer, recorded in the pull request: following GitHub's how-to in the web app, register a GitHub App under the founder's own account, set it up in Settings, and run step 03b's live check with it (sign in to `https://api.githubcopilot.com/mcp/`, list its tools, read one private repository, Remove); then remove the app in Settings and see the agent's row say "Sign in again". Google's card is filled in the same way and saved; its live sign-in is step 08g's, once `google-ads` exists.

## Execution notes

None yet.
