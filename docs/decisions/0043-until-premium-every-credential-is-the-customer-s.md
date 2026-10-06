# 0043. Until Premium, every credential is the customer's

Date: 2026-10-06
Status: accepted (the founder, 2026-10-06: "Remember that Farik is opensource. Any logins or environment variables must be provided by the customer until the last phase where we launch"; asked which phase, the founder answered "Phase 15, Premium", over phase 11, Web launch; the shape below settled by the controller the same day). Amends ADR 0020 (closes its question of a shared client id or the user's own key: the user's own), ADR 0033, ADR 0035 (route 2 becomes the customer's own app; Farik's own apps, the relay and client metadata documents move to phase 15) and ADR 0042 (Google Ads signs in with the customer's own Google app; Google's verification moves to phase 15).

## Context

Farik is open source. The repository was found public on 2026-09-27, and anyone can build the binary. ADR 0035 gave Farik apps of its own for the services that register no client by themselves:
- a GitHub App the founder registers, whose public client id ships in the code (phase 7 step 03b);
- a Google Desktop client in the founder's Google Cloud project, whose client id ships in the code and whose secret is set at build time from `FARIK_GOOGLE_CLIENT_SECRET` (step 08e; ADR 0035's amendment of 2026-10-06);
- for Slack, a relay on AWS that adds Farik's client secret (route 3, deferred to phase 15).

None of them is registered yet: 03b's table ships empty, and 08e's client id is a placeholder.

An app of Farik's own makes Farik's accounts answer for every user:
- Google's verification of the `adwords` scope, its brand verification, and the Google Ads API's access level and quota belong to the founder's project, and every user shares them (step 08e; project plan phase 11).
- GitHub's limit of 50 device-code submissions an hour is per app, and one app would serve every user.
- Any program can use a public client id. Abuse could get the app suspended, and every user's sign-in with it.
- A secret set at build time is in Farik's builds and not in anyone else's, so the open-source build would no longer be the product.

The options for when this ends:
- **Phase 11, the web launch.** The launch would ship Farik's GitHub App and Google app, with Google's verification and a privacy policy as launch dependencies.
- **Phase 15, Premium.** Until then the customer brings every login, and Farik's own apps come with the hosted product. The founder's choice.

## Decision

**Before phase 15, Farik carries no credential of its own that it uses on a customer's behalf.** No build, release, CI workflow, document or default carries one:
- no OAuth client registered to Farik, not even a public client id with no secret;
- no client secret, API key or developer token;
- no build-time secret, `option_env!` included;
- no relay or sign-in service that Farik hosts;
- no client metadata document at an address of Farik's.

**Every sign-in uses what the customer has:**
- route 1, the service registering Farik itself on the customer's computer (step 03);
- route 2 with the customer's own app: an OAuth app the customer registers at the provider, whose client id, and secret where the provider requires one, the customer gives Farik;
- a key the customer pastes (step 01).

**Outside the rule:**
- Farik's own project accounts that the product does not use on a customer's behalf: the website's hosting (phase 11), release signing, the crates.io token, app store accounts.
- Credentials the founder uses as a customer in live tests: the `FARIK_KIT_*_BEARER` variables and the accounts of the milestone runs.

**The customer's own sign-in apps** (phase 7 step 03f, before step 08f). Farik keeps each provider's fixed facts in code: the host it serves or the Farik connector it signs in for, the flow, the endpoints, the scopes, the settings page and a how-to. The customer creates the app at the provider and gives Farik its client id, and its secret where the provider requires one (Google), in the web app or with `farik` on the command line. Client ids are kept in the machine's settings, in the user's Farik state folder. Secrets are kept in the OS keychain. Neither is ever in the team file, the project, an event, a log or an answer on the wire. One exception is unavoidable: the address the browser opens for a loopback sign-in carries the client id, because OAuth puts it there. A grant made with an app the customer removes or replaces lapses, and the agent's page says "Sign in again".

- **GitHub** (step 03b). The customer registers a GitHub App under their own account, with device flow enabled, exactly as 03b's Task 7 described for the founder. The code's flow is unchanged; the client id is the customer's. The founder's live check is done as a customer, with an app of the founder's own.
- **Google** (step 08e). The customer's own Google Cloud project, with the Google Ads API enabled, its OAuth consent screen, and a client of type "Desktop app", with its id and secret. Step 03f removes the build-time secret. Two consequences, which step 03f's readiness review checks against Google's pages of its day: the Google Ads API's access level and quota follow the customer's project, which may start at test access and need the customer to apply for more; and an app in "Testing" status gives refresh tokens that end after 7 days, unless the customer publishes it for their own use. Google's verification of an app of Farik's moves to phase 15.
- **Farik's own connector** (`farik_connector_client` in `crates/core/src/team.rs`). The rule inverts: Farik's own connector signs in with the customer's app for its provider. The team file still takes no client id or port on it, because the customer's app lives in the machine's settings, not in the project.

**Phase 15 gains** Farik's own GitHub App and Google app, offered as a default beside the customer's own, with Google's verification of Farik's app, which uses the homepage and privacy policy of the deferred step 03e; and a client metadata document at Farik's address (step 03's O1). The sign-in relay with Farik's Slack app (ADR 0035, route 3) is a credential of Farik's too, so the Slack integration's steps 02 to 04 wait for phase 15 itself. Its step 01, a key from a Slack app the customer makes, does not.

**Constraints on later phases.**
- Phase 13's receipts intake signs in to the mailbox with an app password or the customer's own OAuth app.
- Phase 14's link from a phone to the customer's computer, and its push notifications, must not need a service Farik holds before phase 15, or those parts wait for it. Phase 14's brainstorm decides which.
- Paid Instagram ads through Meta's server, which admits only clients Meta lists (ADR 0042), wait for phase 15 or for a way to use the customer's own Meta app.

## Consequences

Easier:
- The open-source build is the product: a release carries nothing a build anyone makes does not.
- Phase 11 loses its Google dependencies: the scope's verification, brand verification, the release build's secret, and the Google Ads API's Basic and Standard access.
- Each customer's quotas and limits are their own, and no customer's abuse can get another's sign-in suspended.
- The founder registers no app and holds no secret for the product before phase 15.

Harder:
- Connecting GitHub or Google Ads asks a non-technical user to create an app at the provider. The how-to, shown where a sign-in needs it, is what makes that possible. It is copy Farik must keep true as GitHub and Google change their consoles.
- Google Ads asks the most: a Google Cloud project, the API enabled, a consent screen, a client, and an access level the customer may have to apply for. A customer who leaves the app in Testing signs in again every 7 days.
- A customer's client id and secret are on their computer, the secret in the keychain. Anyone with the customer's computer account can use them. The registration is the customer's own, so its misuse reaches only the customer's app.
- A service that needs a client secret held on a server, such as Slack's official MCP server, has no sign-in before phase 15. It takes the customer's key, or waits.
- Phase 15's own apps carry the lead times ADR 0035 records for Google's verification.
