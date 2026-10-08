# 0048. Farik Cloud is phase 8

Date: 2026-10-08
Status: accepted (the founder, 2026-10-08, in conversation: "Lets do slight modifications to the implementation plan. Lets set up the infra repository as well as the cloud hosting, landing page, etc. in phase 8. Any cloud hosting related planning must be done in the ops repository."; asked which of the web launch's Farik Cloud steps move, "All of them, but the current phase 8 moves 1 phase ahead"; asked whether "cloud hosting" means Farik Cloud's own hosting or also paying customers' cloud workspaces, "Farik Cloud's own"). Numbered 0048 because phase 7 step 11's plan reserves 0045 and step 12d's reserves 0046 (ADR 0047). Amends ADR 0044 (its free tier starts in phase 8, not at the web launch), ADR 0047 (`farik-ops` is set up in phase 8 step 01), ADR 0017 (Farik's hosting starts in phase 8), and every ADR that names a phase after phase 7 by its number: the phases from Engines and providers on move up by one.

## Context

Since ADR 0044 (2026-10-06) Farik Cloud's free tier, the sign-ins through Farik's own apps with a Farik account, started at the web launch, phase 11, as its steps 01 to 01f: the website and domain (01), the privacy policy (01b), the Farik account (01c), the sign-in relay (01d), signing in through Farik Cloud (01e), and the founder's live checks moved from phase 7 (01f), with Google's verification of the `adwords` scope and the Google Ads API's access level before them. Since ADR 0047 (2026-10-08, earlier the same day) Farik Cloud's service and everything Farik hosts are planned separately in `farik-ops`, Farik's private operations repository, and this repository keeps the app's side.

The order after phase 7 was: 8 Engines and providers, 9 Ecosystem, 10 Proof of concept, 11 Web launch, 12 Business workspaces, 13 Desktop, 14 Native mobile, 15 Premium. So Farik Cloud waited for three phases that do not need it, and phase 7's live checks that wait on it (GitHub through Farik's GitHub App, Google Ads through Farik's Google app, and the kit check's Google Ads part) waited with it. The proof of concept's benchmark ran its kits without Google Ads for the same reason.

On 2026-10-08 the founder, who has bought the domain and is setting up operations, asked for the infrastructure repository, the cloud hosting and the landing page in phase 8, and for every cloud hosting plan to be in the operations repository. Asked which of the web launch's Farik Cloud steps move, the founder answered "All of them, but the current phase 8 moves 1 phase ahead". Asked whether "cloud hosting" means Farik Cloud's own hosting or also paying customers' cloud workspaces, the founder answered "Farik Cloud's own".

The options were:
- **Keep Farik Cloud at the web launch.** The founder declined it.
- **Move only the hosting and the site to phase 8, and keep the sign-ins at the launch.** The founder answered "All of them".
- **Move every Farik Cloud step into a new phase 8, after the role kits, and move the phases after it up by one.** The founder's choice.

## Decision

**A new phase 8, Farik Cloud, comes right after phase 7, Role kits.** It sets up `farik-ops`, Farik's private operations repository (ADR 0047), and Farik Cloud's own hosting on AWS (ADR 0017), then the website on Farik's domain, the privacy policy, the sign-in relay, the Farik account, signing in through Farik Cloud from the app (GitHub in a single click, and Google Ads), and last the founder's live checks moved from phase 7, after Google's verification of the `adwords` scope and the Google Ads API's access. Its steps:
- 01, the `farik-ops` repository and Farik Cloud's hosting: the accounts, the domain and the environments;
- 02, the website and domain, phase 11's step 01 until now;
- 03, the privacy policy, 01b until now;
- 04, Farik Cloud's sign-in relay, with Farik's apps registered, 01d until now;
- 05, the Farik account, 01c until now;
- 06, signing in through Farik Cloud, 01e until now;
- 07, phase 7's live checks, 01f until now.

**All cloud hosting planning is in `farik-ops`.** In this repository each hosted step (01 to 04, and the account service of 05) is a row that says it is planned separately in `farik-ops` (ADR 0047), with no detail of how it is hosted. Only the app's side is planned here: the Farik account's app side, signing in through Farik Cloud, whose relay API phase 7 step 03d holds as the contract, and the live checks run in the app.

**The phases after it move up by one:** 9 Engines and providers, 10 Ecosystem, 11 Proof of concept, 12 Web launch, 13 Business workspaces, 14 Desktop, 15 Native mobile, 16 Premium. The web launch keeps its other steps, renumbered from 01: its release step, step 02 until now, is step 01. Farik Cloud's free tier, the website and the privacy policy are phase 8's and already run at the launch.

**Cloud hosting means Farik Cloud's own.** Paying customers' cloud workspaces, and the paid tier that runs their agents, stay in Premium, now phase 16, planned separately in `farik-ops` as ADR 0047 says.

ADR 0044's title stays: it records the decision of its day, with a dated line pointing here. The other ADRs that name a moved phase by its number keep their text and carry a dated line.

## Consequences

Easier:
- Phase 7's live checks that wait on Farik Cloud run in phase 8, months before the launch rather than in it: GitHub through Farik's GitHub App (step 03b's), Google Ads with a small budget (steps 08e to 08g's), and the kit check's Google Ads part (step 13's).
- The website and the privacy policy are live before the proof of concept and the launch, so Google's verification of the `adwords` scope, with its lead time of days to weeks, runs during phases 9 to 11 instead of blocking the launch.
- Engines and providers (phase 9) and the proof of concept (phase 11) can test with GitHub and Google Ads signed in. ADR 0044's reason for running the benchmark's kits without Google Ads, that no customer signs in to Google before Farik Cloud runs, is gone; the proof of concept's brainstorm decides whether its kits include Google Ads.
- The launch carries no cloud work of its own: its readiness is the release, the install path and the review of the approved sites.

Harder:
- Farik runs and pays for a cloud service through three phases before any customer uses it, with the costs and the attack surface ADR 0044 records for Farik Cloud (the tokens it sees in transit, the account's personal data, Farik's app registrations that every customer depends on).
- The launch cannot happen before phase 8 lands, and phase 8's hosted steps are tracked in `farik-ops`, so the order of this repository's phases depends on work a readiness review here cannot read.
- Every phase after 7 is renumbered, so the project plan, the spec, the design documents, the phase 7 step plans, `CLAUDE.md` and a few code comments change their forward pointers. The renumber changes no behaviour, so `docs/SPEC.md` takes no revision for it, as ADRs 0025, 0029 and 0040 did; the ADRs and the revision notes keep the numbers of their day.
- Copy written for "the web launch" as the start of Farik Cloud, such as the kit row's "{service} comes with Farik’s web launch." (spec 6.7, phase 7 step 08f), now names a later moment than Farik Cloud's. It is tested product copy, so it is changed, if at all, by a step of its own.
