# 0047. Farik Cloud is planned separately in farik-ops

Date: 2026-10-08
Status: accepted (the founder, 2026-10-08, in conversation). Numbered 0047 because phase 7 step 11's plan reserves 0045 (`0045-devops-deploys-watching-and-incidents.md`) and step 12d's reserves 0046 (`0046-fixed-settings-and-file-keys-in-a-kit-server.md`). Amends ADR 0017 and ADR 0044 (where what Farik hosts, and Farik Cloud's service, are planned) and ADR 0035 (where the relay's server side is planned).
Amended 2026-10-08 by the founder's two answers of the same day: the website's code (homepage, downloads, privacy page) lives in `farik-ops` too ("farik-ops"), and the paid cloud's code that runs customers' workspaces is private there as well, not only its hosting ("Code private too"); this repository keeps only the app's side of connecting to it.

## Context

ADR 0044 made Farik Cloud a service Farik runs from the web launch: a free tier that signs customers in through Farik's own apps, with a Farik account, and a paid tier in phase 15. ADR 0017 had already put what Farik hosts on AWS, and the plans that followed (phase 7 steps 03c and 03e, the project plan's phases 11 and 15, and `docs/SPEC.md`) set out the service's server side and its hosting in this repository: the relay's stack, its secrets and limits, the website's and the privacy page's hosting, the domain's and the mailbox's records, and the registration of Farik's apps.

The repository is public, and has been since 2026-09-27. On 2026-10-08 the founder, who has bought the domain and is setting up operations, said: "I dont want the planning for the cloud hosting to be on the public repo". Asked whether the repository should stay public, the founder said: "Keep it public but just modify the files to say plan separately in farik ops".

The options were:
- **Keep every plan here.** The founder declined it.
- **Make the repository private.** The founder declined it: Farik is open source.
- **Remove the planning from history.** Not asked for; the history stays as it is.
- **Plan the cloud's side in a private operations repository, `farik-ops`, and point to it from here.** The founder's choice.

## Decision

**Farik Cloud's service and everything Farik hosts are planned separately in `farik-ops`**, Farik's private operations repository: the cloud accounts and stacks, the relay's server side, the Farik account service, the hosting of the website and the privacy page, the domain's DNS and mail records, the registration and verification of Farik's GitHub App and Google app as operations, Slack's app and listing as operations, and the paid cloud hosting of phase 15. In this repository each of those plans becomes a short pointer: "planned separately in `farik-ops`".

**The app's side stays here, in public:** the open-source code that signs in through Farik Cloud and keeps the tokens on the user's computer, the Farik account's session in the app, the screens, the founder's live checks in the app, and what Farik Cloud does for the user, which `docs/SPEC.md` keeps describing (sign-ins go through it, the tokens are handed back and kept on the user's computer, it keeps none, a Farik account, the free tier is sign-ins only).

**The boundary is the contract between the app and the cloud.** The API the app calls is written here, in phase 7 step 03d, the app's side, because the open-source code is built against it. A change to it is made in both repositories.

**No secret is in either repository's files** (ADR 0043). `farik-ops` being private does not make it a place for a client secret, a key or an account's credentials: those stay in the cloud's secret store and the founder's own keeping.

ADRs 0017, 0035 and 0044 stay as the decisions of their day, each with a dated line pointing here. Nothing is removed from git history.

## Consequences

Easier:
- The founder plans and runs operations in private, with account details beside the plans, while the product stays open source.
- ADR 0017's local, gitignored `deploy/plan.md` has a proper home: the deployment plan lives in `farik-ops`.
- The public plans hold only work this repository does, so a contributor reads no plan for code they cannot see.

Harder:
- The contract has two readers. The app's side of a sign-in (step 03d, phase 11 step 01e) is planned here and the service's there, so a change to the API needs a commit in each, and a readiness review here cannot read the service's plan.
- Some of the cloud's facts the app depends on (the service's address, its limits, which apps it holds, Google's verification status) are decided in `farik-ops`. The founder gives each, public ones only, when a step here needs it, as the founder already gives a client id or a domain in conversation.
- The earlier plans stay readable in this repository's history, so what was planned here before 2026-10-08 remains public.
- The project plan's phase 11 and 15 rows no longer say everything those phases deliver: their cloud rows are pointers, and the launch's readiness depends on work tracked elsewhere.
