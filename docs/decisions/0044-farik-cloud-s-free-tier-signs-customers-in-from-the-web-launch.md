# 0044. Farik Cloud's free tier signs customers in from the web launch

Date: 2026-10-06
Status: accepted (the founder, 2026-10-06, asked to approve phase 7 step 03f's boards: "The github login must be done through a single click. Lets chat about this before moving forward"; after GitHub's manifest flow was discussed: "Lets change the customer's own app. Lets provide free cloud hosting tier for customers that will allow them to sign in from their local machine into the cloud that is managed by farik."; then four answers the same day: what the free tier does, "For free tier there will only be sign-ins. For paid customers we can provide full cloud support from running agents, workspace, google and github apps, etc."; an account, "Yes, a Farik account"; when, "At the web launch"; step 03f, "Drop it". The shape below settled by the controller the same day). Supersedes ADR 0043 in part: its route of the customer's own app, and its phase 15 date for Farik's own apps; its rule that the code carries no credential of Farik's stands, and now holds for good. Amends ADR 0017 (Farik hosts a third thing, Farik Cloud, from phase 11), ADR 0020 and ADR 0033 (where ADR 0043 amended them), ADR 0035 (routes 2 and 3 are Farik Cloud's from phase 11; GitHub takes a pasted key before then) and ADR 0042 (Google Ads signs in through Farik Cloud's Google app from phase 11).
Amended 2026-10-06 by phase 7 step 07c (the founder's O1): the Product Manager files GitHub issues and comments, each asking first, so Farik's GitHub App asks for more than read-only permissions. See "Amendment of 2026-10-06" at the end.
Amended 2026-10-08 by ADR 0047 (the founder: "Keep it public but just modify the files to say plan separately in farik ops"): Farik Cloud's service and hosting are planned separately in `farik-ops`, the private operations repository; the app's side stays here, and the API between them is written in phase 7 step 03d. This record stays as the decision of its day.
Amended 2026-10-08 by ADR 0048 (the founder: "Lets set up the infra repository as well as the cloud hosting, landing page, etc. in phase 8"; of the web launch's Farik Cloud steps, "All of them, but the current phase 8 moves 1 phase ahead"): Farik Cloud's free tier starts in a new phase 8, Farik Cloud, right after the role kits, not at the web launch, which is now phase 12. Phase 11's steps 01, 01b, 01d, 01c, 01e and 01f are phase 8's steps 02 to 07, after step 01 sets up `farik-ops` and the hosting; the founder's live checks moved from phase 7 run in phase 8 step 07, after Google's verification and the Google Ads API's access, which are no longer the launch's dependencies; the paid tier is Premium, phase 16, and the phases after phase 7 moved up by one. The title and the text below stay as decided, with the numbers of their day.

## Context

ADR 0043, earlier the same day, kept every credential the customer's until phase 15. A customer connecting GitHub or Google Ads would register an app of their own at the provider, from a how-to, and give Farik its client id and secret (phase 7 step 03f, drafted, its boards drawn). Shown the boards, the founder asked for GitHub's sign-in to be a single click.

Registering a GitHub App is not one click, even from a manifest that fills in its settings. GitHub's device flow (step 03b) asks the user to type a code. A sign-in of one click needs an app that is already registered, Farik's, and GitHub's web flow, whose code exchange needs the app's client secret (ADR 0035). Farik is open source, so that secret cannot ship in the code (ADR 0043). A server Farik runs can hold it, as ADR 0035's relay holds Slack's.

The options were:
- **The customer's own app** (ADR 0043, step 03f). Nothing of Farik's is hosted, but each customer registers an app at each provider; Google's needs a Cloud project, a consent screen, a client and an access level to apply for. The founder dropped it.
- **Farik's apps in the code.** Rejected by ADR 0043 and again here. A secret in a public repository or a public binary protects nothing; GitHub's push protection reports a Google secret to Google; and Farik's builds would differ from the open-source build.
- **Farik's apps held by a cloud service Farik runs, from the web launch.** The founder's choice. The service is ADR 0035's stateless relay, generalized from Slack to every app Farik registers, with an account per customer so that its use has limits.
- **The same, but only in phase 15, Premium.** ADR 0043's date. The founder moved it to the launch and made it free.

## Decision

**Farik Cloud** is a service Farik runs on AWS (ADR 0017), from the web launch, phase 11. It has two tiers:
- **Free: sign-ins only.** It holds Farik's registered apps and their secrets, server side: Farik's GitHub App; Farik's Google app, for the one scope `https://www.googleapis.com/auth/adwords` (ADR 0042); and Slack's app when phase 15's Slack integration comes. A customer's local Farik starts each sign-in through it and refreshes each grant through it. Farik Cloud adds the secret and hands the tokens straight back to the customer's computer. It stores none. They are kept on the computer, in the OS keychain, as every grant is (ADR 0033). This is step 03c's relay design, generalized from Slack to every registered app.
- **Paid: the full cloud** (phase 15, Premium): running agents, workspaces, and the rest of spec 9's premium list, with the same apps.

**A Farik account.** The customer signs in to Farik Cloud once per computer, in the browser. Free-tier sign-ins need it. Fair-use limits are kept per account, and it is the account that later upgrades to Premium. The computer keeps the account's session as the customer's own credential, in the OS keychain, as it keeps the AI account's (ADR 0021). The local Farik reaches Farik Cloud with that session alone: no key, token or id of Farik's is in the binary for it. How the account signs in (an emailed code, or Google or GitHub) is phase 11's brainstorm, not decided here.

**The open-source code never holds a credential of Farik's**, before or after the launch: no client secret, no API key, no developer token, and no build-time secret, `option_env!` included. In step 03c's design Farik Cloud answers the authorization address, its client id included, when a sign-in starts, so the code needs no client id of Farik's either. Farik Cloud's secrets live in AWS Secrets Manager, readable by its own functions alone. ADR 0043's rule becomes:
- until the web launch, nothing that needs a registered app of Farik's is offered;
- from the launch, Farik Cloud provides those apps;
- the code never does.

Outside the rule, as in ADR 0043: Farik's own project accounts that the product does not use on a customer's behalf (the website's hosting, release signing, the crates.io token, app store accounts), and the credentials the founder uses as a customer in live tests.

**Before phase 11:**
- **GitHub's MCP server** (`https://api.githubcopilot.com/mcp/`) is connected with a key the customer pastes, a fine-grained personal access token, as Slack's bridge is (phase 15 step 01). GitHub's server takes one as `Authorization: Bearer` (its README, read 2026-10-06); with a fine-grained token every tool is listed and GitHub enforces the token's permissions (GitHub's changelog, 2026-01-28). With step 03b's table empty, `ConnectorAdd` already falls to the key fields for that address. GitHub's own page lists "Access to Copilot" as a prerequisite, so whether an account without Copilot can use the server stays open, as step 03b's live check left it.
- **No kit ships GitHub's server.** Steps 06 and 07 left GitHub Issues and private code search for step 03b's sign-in, because a pasted GitHub key went against ADR 0035. The kit format takes a pasted key (`headers`, `credential_keys`, `key_page`). The founder decided the same day that the Product Manager's and the Architect's kits gain GitHub's server with a pasted fine-grained token before the launch ("Yes, pasted token"), replaced by the sign-in through Farik Cloud at the launch; phase 7 step 07c plans it.
- **Google Ads** (phase 7 steps 08f and 08g) is built and tested in phase 7 against the fake Google Ads server, its tests signing in with tables of their own, as step 08e's do. No customer can sign in to Google until Farik Cloud runs, since no build has a Google entry. So the founder's live checks of step 08e's sign-in, of 08f and of 08g, and the Google Ads part of the kit check (step 13), move to phase 11.
- **The founder's single click is not met before the launch.** A pasted token is not one click; the founder accepted that in choosing the launch for Farik Cloud. From phase 11 GitHub signs in through Farik Cloud. ADR 0035 already said that GitHub "moves to the relay with the web flow" as sign-ins near the device flow's cap of 50 an hour per app, which one app of Farik's for every customer would reach; the web flow is also the one with no code to type. Phase 11's brainstorm settles GitHub's flow, with the founder's single click as its requirement.

**Step 03f is dropped** (the founder: "Drop it"). Its plan stays as the record, marked dropped. Its unapproved boards are deleted.

**What moves to phase 11**, as Farik Cloud's free tier, planned there from its own brainstorm:
- the Farik account;
- the sign-in relay (phase 7 step 03c), Farik signing in through it (step 03d), and the homepage and privacy policy (step 03e), generalized from Slack to GitHub and Google, with the account added; their plans move there as they are and are re-planned;
- Farik's GitHub App (step 03b's Task 7) and Farik's Google project, consent screen and client (step 08e's Task 6), registered by the founder, with their secrets set in Farik Cloud and never in a build;
- Google's verification of the `adwords` scope, which needs the homepage and the privacy policy on Farik's verified domain, brand verification, a demo video and Google's review time. Until it is verified, Farik's Google app signs in only named test users, whose sign-ins end after 7 days;
- the Google Ads API's access for Farik Cloud's Google project: Explorer, then Basic after brand verification, then Standard;
- the founder's live checks moved from phase 7: step 03b's, steps 08e to 08g's, and the kit check's Google Ads part.

Client metadata documents (step 03's O1), which ADR 0043 moved to phase 15 because they sit at an address of Farik's, may come with Farik Cloud's site; phase 11's brainstorm decides.

**What stays in phase 7.** Step 03b's device flow, step 08e's loopback sign-in with PKCE and its refresh, and the table's matching rules stay in the code: phase 11's sign-in through Farik Cloud builds on them. Step 08e gains a last task, Task 8, which removes the build-time secret and the Google entry built from it, test first. It runs after step 09c.

**What stays in phase 15.** The paid cloud, and the Slack integration: Slack's own app, its entry in Farik Cloud, and its Marketplace listing. The integration's step 01, a key from a Slack app the customer makes, is unchanged.

**Later phases** (ADR 0043's constraints, revised):
- Phase 13's receipts intake signs in to the mailbox with an app password, or through Farik Cloud where a provider's sign-in suits. The customer's own OAuth app is no longer a route.
- Phase 14's link from a phone to the customer's computer, and its push notifications, are not sign-ins, so the free tier does not carry them. They need no service of Farik's, or they are paid, or the founder widens the free tier; phase 14's brainstorm decides which.
- Paid Instagram ads through Meta's server, which admits only clients Meta lists (ADR 0042), wait for Meta to list Farik's client, which Farik Cloud would hold.

## Consequences

Easier:
- From the launch, a customer connects GitHub and Google Ads by signing in, with no app to register, no client id to copy and no secret on their computer. GitHub's sign-in can be the single click the founder asked for.
- The open-source build stays the product. No build carries a secret of Farik's, and Farik's builds are no different from anyone's.
- The Farik account answers ADR 0035's "anyone can call the relay with Farik's Slack app": a call needs an account, limits are per account, and an account that abuses Farik Cloud can be stopped without stopping the app for everyone.
- Phase 7 needs no Google Cloud project, GitHub App or AWS account of the founder's.

Harder:
- **Farik runs a cloud service from the launch**, with ADR 0035's costs and limits for the relay (about $10 a month at launch traffic, one region, the founder's AWS account and domain), plus the account's sign-in and its store, which are new. When Farik Cloud is down, sign-ins and refreshes through it wait; a pasted key still works where the service takes one.
- **Farik Cloud sees each token in transit**, once per exchange and once per refresh, as ADR 0035 records for the relay. A Farik Cloud taken over could copy the tokens of the customers who sign in or refresh while it is. It now holds GitHub's and Google's apps as well as Slack's. The defences are ADR 0035's.
- **Farik Cloud holds personal data**: each account's email address, and whatever the account's sign-in needs. The privacy policy must say so, and an account must be removable on request. Phase 11's brainstorm plans both.
- **The launch has Google dependencies again**, as it had before ADR 0043: Google's verification of the `adwords` scope and its lead time (days to weeks, once the homepage and privacy policy are live), brand verification, and the Google Ads API's access levels. Until the verification, only named test users sign in to Farik's Google app.
- **Quotas are shared.** The Google Ads API's access level and quota belong to Farik Cloud's Google project, and every customer shares them: Explorer allows 2,880 operations a day, and step 08g's spend read uses 96 a day per active plan. GitHub's 50 device-code submissions an hour, if the device flow stays, are one app's for every customer. Both are phase 11's to plan for: the access levels to apply for, and the limits per account.
- **Every customer depends on Farik's app registrations.** If GitHub or Google suspends an app of Farik's, every customer's sign-in to that service stops until it is restored. A pasted token is GitHub's fallback; Google Ads has none.
- **Before the launch, GitHub needs a pasted token, and Google Ads cannot be signed in to at all.** The founder's live checks of Google Ads, and the kit check's Google Ads part, run only in phase 11. Phase 10's benchmark runs its kits, on both sides, without Google Ads.
- **The `google-ads` connector ships in the Marketing Specialist's kit before anyone can sign in to it.** With no Google entry in any build, `KitConnect` answers `sign_in_not_supported`, whose words are written for a service that cannot sign in. Step 08f's readiness review settles what the kit's row says until the launch, that Google Ads comes with Farik's web launch, rather than leave the generic sentence.
- Step 03f's design work, its boards and how-tos, is spent.

## Amendment of 2026-10-06

Phase 7 step 07c (the founder's O1, in conversation: the Product Manager files issues and comments on GitHub, each asking first, "Files, asking first"). The kits' GitHub is not read-only for the Product Manager:
- Its pasted key holds repository Issues read and write, and, for an organisation's boards, organisation Projects read. The Architect's holds repository Contents and Pull requests read.
- From phase 11, Farik's GitHub App asks for repository Contents and Pull requests read, Issues read and write, and organisation Projects read. This replaces ADR 0035's "read-only repository permissions" for GitHub; otherwise the Product Manager's two writes would stop at the switch from the key to the sign-in.
- Each write still waits for the human, with no allowance (ADR 0037): an issue or a comment is published to everyone who can see the repository.
