# Phase 7, step 10b2: Approved sites

Status: draft. Its readiness review (ADR 0032, one round) runs once step 10b has landed; the founder answered O1 to O3 on 2026-10-07.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.6, 5.7, 6.10, 8.2, 8.5, 8.6; F9
Depends on: step 10b of this phase (the role, `Role::ProcurementSpecialist`, its folder `crates/roles/roles/procurement_specialist/` and tools; `TOOLS` with 30 read-tier entries and `daemon/mcp.rs`'s count at 37, which this step's numbers assume); step 08c (a task waiting for the owner: `open_plans`, migration 0013, `human_message`'s owner decisions, `farik_request_transition`'s refusal while a plan waits); step 05 (the kit loader, `load_kit`, the pattern for a shipped file held to its schema); step 02 (decisions only on `POST /command` and the browser's RPC, `decide_tool_call`'s one lock, `waiting.list` rows); phase 6 step 12 (the preview's `url` check, `check_urls` in `crates/core/src/governor/permissions.rs`); phase 6 (merged in #19)
Readiness confirmed by: not yet
Decided by the founder, 2026-10-07, in conversation (step 10b's readiness review): asked whether to accept and record that the agent reads untrusted sellers' pages while `network` lets it fetch any address, "or restrict its web access (it may only browse addresses you approve)?", the founder answered "Restrict its web access". ADR 0039 is amended the same day.
Decided by the founder, 2026-10-07, in conversation (this plan's O1 to O3): how long an approval lasts, "Until you remove it"; whether the owner may add a site the agent never asked for, "We will compile a list of approved websites prior to launch and add it as farik approved websites. The user may add more if he chooses to"; whether the role starts with any site allowed, "Choose trusted shops across different products categories." ADR 0039's amendment records them.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Split from step 10b, whose readiness review raised it; it lands before step 10c and before the kit (10d), and the role is not used live until it has.

## Goal

The Procurement Specialist reads only Farik's approved sites and the sites the owner allowed. Farik's approved sites are long-established shops across the categories a small business buys in, shipped with Farik and open from the agent's first task; the owner may turn any of them off. When the agent needs another seller's or maker's site, it asks for it with the reason, the owner allows it or not on Today, and an allowed site stays allowed until the owner removes it (O1); the owner may also add a site the agent never asked for (O2). The agent's page lists both. A page the agent reads can no longer have it send the business's quotes, prices or notes to an address the page chose, because the agent cannot fetch any address on a site not on the list. It still searches the web freely. Out of scope: any other role's web access (the Finance Specialist's risk was accepted and recorded on 2026-10-07, spec 8.6); the kit's connectors themselves (10d, held by this step's rule); approving pages one by one; the list's final contents, which phase 11 step 02 reviews before the launch.

## Decisions

- **Who is held: a role, fixed at session start.** In a new `farik_core::governor::sites`: `pub enum WebAccess { Open, ApprovedSites }` and `pub fn web_access(role: Role) -> WebAccess`, `ApprovedSites` for `Role::ProcurementSpecialist` and `Open` for every other role. `SessionRegistration` (`crates/runtime/src/daemon.rs`) gains `pub web: WebAccess`, set from the agent's role where a session is registered (`orchestrator/session.rs`), whatever its purpose, so the hook reads no team for it; a `conversation` session holds the agent's tiers and is held by this, while a `chat` or `explore` session holds `read` alone, so its `WebFetch` is already refused by tier. The role keeps `network` (spec 6.10); this narrows what it reaches. Rejected: a tier of its own, which would change the tier table, the agent page and every tier test for one role; keying on the purpose, since a chat reads the web too.
- **What a site is.** A host in the ASCII form the `url` crate gives (`url = "=2.5.8"`, already the workspace's, added to `farik-core`: it parses and does no I/O, so `cargo xtask core-io` passes): lower case, an internationalised name in its punycode `xn--` form. `pub fn site_of(address: &str) -> Result<String, SiteFault>` accepts an address that parses as a URL with scheme `https`, no user name or password, no port other than 443 (`url` drops a default port, so `:443` is none), and a host that is `url::Host::Domain` (an IPv4 or IPv6 address is never a site) with at least one dot and at most 253 characters once one trailing dot is removed (`https://shop.example./` is `shop.example`); it then drops one leading `www.` label when what remains still has a dot (`https://www.Shop.example/` is `shop.example`; `https://www.com/` stays `www.com`). So a host and its `www.` twin are one site in every request, decision, list entry and fold, a rule of Farik's own, since a `www.` name belongs to whoever holds the name under it, and an address is on an approved site when its `site_of` is in the approved set, nothing more: `shop.example.com` is not `example.com`'s, because one name often holds many owners' sites (`*.myshopify.com`, `*.github.io`, `*.blogspot.com`), and a subdomain is asked for as its own site. Path, query and fragment are not judged: a site is approved whole, since a seller's prices and terms are on many pages. Rejected: `http` (Claude Code's `WebFetch` upgrades it to `https`, so it would be judged as one address and read as another; the agent writes `https`); approving subdomains with their parent; approving each page (the owner would be asked on every link).
- **The role starts with Farik's approved sites (O3).** Farik ships a list of long-established shops, each with its official primary domain, across the categories a small business buys in (below), open to every team's Procurement Specialist from its first task with no request. It is public data, not a secret: ADR 0044 keeps Farik's credentials out of the code, and a list of shops is none. The file is `crates/roles/roles/procurement_specialist/approved_sites.yaml`, a mapping with one key, `sites`, a list of `{ host, shop, category }`, held to a new `docs/schemas/approved-sites.schema.json` (`host` a string; `shop` 1 to 60 characters on one line; `category` one of the ten ids below; no other key) and embedded with `include_str!`, as `kit.yaml` is. `farik_roles::sites::parse_farik_sites` holds it to the schema, then refuses, naming the entry, a host that `site_of("https://<host>/")` does not give back unchanged (so each is a bare `https` host in lower-case ASCII, punycode for another alphabet, a domain with a dot, with no scheme, path, port, IP address or leading `www.`) and a host listed twice. `farik_roles::sites::farik_sites()` parses the shipped file once (`LazyLock`, its `expect` naming the test that rules the failure out). The list is the first version, compiled on 2026-10-07; the launch review (phase 11 step 02) finalizes it.
- **A release's list reaches every team; the owner's choices stay (O1).** The list is read at run time, never copied into the log. The approved set is the fold, in sequence order, of `site.approved` (adds its host) and `site.removed` (takes it away) in the project's event log (`.farik/local/farik.db`, never committed), starting from Farik's hosts of the running release: `pub fn approved_sites(log: &EventLog, farik: &BTreeSet<String>) -> Result<BTreeSet<String>, StoreError>` in a new `farik_store::sites` (which does not depend on `farik-roles`; the runtime passes the hosts), read through the kind index (`events_by_kind`). So a site a release adds is open on upgrade; a site a release drops is closed unless the owner allowed it themselves; a site the owner turned off stays off, its `site.removed` in the log; a site the owner allowed stays allowed "Until you remove it". The hook reads the set at each `WebFetch` and connector call of a held session, under the sessions lock, as `grant_for` reads a task's events there: two kinds, a few dozen events. Rejected: a copy held by the daemon, as `allowance_counts` is, a second state to keep in step with the log for no measured cost. A project has one team, so the set is the team's, shared by every Procurement Specialist on it. Rejected: `team.yaml`, which is committed and travels with the repository, so a pull could add a site the owner never allowed (8.6: a committed team file is untrusted); a file under `.farik/local/`, a second record to keep in step with the log; copying Farik's list into the log at the team's start, which would freeze it at that release. ADR 0034's confirmation of a skill in this computer's log is the precedent.
- **How the agent asks: `farik_request_sites { sites: [{ url, why }] }`**, `read` tier, offered to an `implement` session about its own task of an agent whose `web` is `ApprovedSites`, the one session that can wait for the owner. 1 to 10 entries; `url` is the first page it wants, which `site_of` must accept (`site_invalid: <url> <why>`); `why` is 1 to 300 characters on one line (`site_why_invalid`). For each entry: a site already approved, one of Farik's included, is answered `allowed`, and one already waiting for this task `waiting`, nothing recorded; otherwise `site.requested { host, url, why }` is recorded with the task, agent and session on its envelope and answered `asked` with its request number, the event's seq. A site of Farik's the owner turned off is asked for as any other. At most 20 requests wait in the project at once (10f's cap on drafts is the precedent): an entry past it is answered `full`, and a call that records nothing because of it is refused `too_many_site_requests`. When anything was recorded the answer ends `next: "end your turn: the owner's decision starts the next session"`. Rejected: one site per call (a search finds several sellers at once); having the hook ask on a refused `WebFetch`, as a connector call asks (ADR 0031), which would stop the session at the first link it tried and ask about pages it only glanced at.
- **`farik_read_sites {}`**, `read` tier, offered with `farik_request_sites` and in the role's chat: `{ approved: [{ host, shop?, category? }], waiting: [host], declined: [{ host, note }] }`, `shop` and `category` for Farik's sites, `waiting` and `declined` those of the session's task (none without one), so a new session knows where it may read, and which shop sells what, without a refused call.
- **The task waits, as for a marketing plan (08c).** Migration `crates/store/src/migrations/0014_site_requests.sql` (the next free number at HEAD) adds `open_sites INTEGER NOT NULL DEFAULT 0 CHECK (open_sites >= 0)` to `task_projections`; `site.requested` raises it on its task, and a `site.approved` or `site.declined` carrying `request` lowers it on its own task, where each is recorded. The task waits on the human while `open_questions`, `open_approvals`, `open_plans` or `open_sites` is above zero (`projections.rs`), so no session starts for it and the board marks it "Waiting on you". While a request of its task waits, `farik_request_transition` refuses `verifying` with `site_request_waiting: <host> waits for the owner; end your turn`, recording nothing. The next session about the task is told, as the human's message (`human_message`, `orchestrator/messages.rs`), each decision on its requests since its last session started: "The owner allowed you to read <host>." with " The owner adds: <note>" when there is a note, or "The owner did not allow <host>: <note>" ("The owner did not allow <host>." without one); a note is the owner's own words, not wrapped (ADR 0011), and a decision whose envelope names an agent or a session is not the owner's and is not told. Rejected: holding no task, as a post outside the plan does (6.5), since the work needs the site.
- **What the hook does.** For a session whose `web` is `ApprovedSites`, `judge_call` (`daemon/hooks.rs`) checks a `WebFetch` after its tier: its `url` must be a string whose `site_of` is in `approved_sites(log, farik hosts)`, read at the call; otherwise `site_not_approved: <host> is not a site the owner allowed; ask with farik_request_sites, then end your turn`, or, for an address `site_of` refuses, `site_not_approved: <url> is not an https address on a named site`. The session goes on. For a connector's call, `judge_connector` adds, right after the preview's `url` check and before the Designer's plan gate, the 64 KiB limit, a grant and an allowance, whatever the tool's tag, so that neither a grant nor an allowance takes a call to an unapproved site, `pub fn check_site_urls(input: &serde_json::Value, approved: &BTreeSet<String>) -> Result<(), SiteRefusal>`, the preview's `check_urls` generalised: every field named `url` must be a string, and every field named `urls` an array of strings, at any depth, each passing the same test (`site_not_approved`). A removal or a turn-off takes effect at the next call; what the agent already read stays in its session. `WebFetch` (Claude Code 2.1.285) returns a redirect to another host to the model instead of following it, so the address it moves to is the agent's next `WebFetch`, judged again. Rejected: judging after the fetch (`PostToolUse` cannot unsend a request).
- **`WebSearch` stays open.** It is judged by the `network` tier alone, as for every role. Its query goes to the one search service Claude Code uses, through the model provider, which already receives every word of the session; neither a page nor the agent chooses where it goes, so it cannot carry data to a seller or to an address a page names. Its results are titles and addresses, untrusted, and reading one is a `WebFetch` the list holds. The query does leave the computer, as every prompt does; 8.6 says so. Rejected: refusing it (the agent could not find the sellers to ask about); asking the owner per query (a query reaches no seller).
- **Connectors.** A user's own server given to the role, and its kit's connectors (10d), are held by their `url` and `urls` fields; an address in a field of another name is not judged, which 8.6 records. Step 10d checks, when it pins Exa, that `web_fetch_exa` takes its address in a `url` or `urls` field, and tags it `denied` otherwise.
- **Under `auto`, a request still waits for the owner** (ADR 0041, amended 2026-10-07). The list is a limit the owner sets, like a spending limit, not an outward act; under `auto` the limits are the only guard against a steered agent (ADR 0041's Consequences), and approving requests on their own would make the list as wide as any page asks.
- **Only the owner decides.** Three commands, on `POST /command` behind the daemon's token and in the browser's RPC behind the session cookie, as `tool_approve` (8.6); no Farik tool decides, adds or removes a site, and the role has no `execute`, so the no-sandbox residual of `daemon.json`'s token (8.6) does not reach it. `site_decide { request, allow, note? }` (`note` at most 600 characters) records `site.approved { host, request }`, then the same for each other request still waiting for that host, each on its own task, or `site.declined { request, host, note? }`; a seq that is no `site.requested` is `unknown_site_request`, and a second decision `site_request_decided`, the check and the write under one lock as `decide_tool_call`'s. `site_add { site }` (O2: "The user may add more if he chooses to") takes a host or an address (`https://` put before a bare host, then `site_of`; `site_invalid`), records `site.approved { host }` with no request and no task, settles each request waiting for it as above, and is `site_already_allowed` for an approved host; it also turns one of Farik's sites back on. `site_remove { host }` records `site.removed { host }`, `site_not_allowed` for a host not approved; for one of Farik's sites it is the turn-off.
- **The owner may turn off one of Farik's sites** (decided here, as the founder's O3 answer leaves it open): Farik vouches for the list, the owner may not want a shop, and the switch costs no new event or command. It is a `site.removed` kept in the log and shown on the agent's page, and it outlasts upgrades (above). Rejected: Farik's sites fixed on, which would make a shop the owner distrusts one the agent reads anyway.
- **Events**, four kinds, `<entity>.<past_tense_verb>`: `site.requested { host, url, why }` (task, agent, session on the envelope); `site.approved { host, request? }` and `site.declined { request, host, note? }` (on the request's task, or on none for an added site; no agent, no session); `site.removed { host }` (no task). In `event.schema.json` and every exhaustive match: `EventBody`, `kind`, `body_def_name`, `EVERY_KIND` (`crates/protocol/src/event.rs`), the names (`protocol/src/lib.rs`), the fixtures.
- **Queries and lists.** `sites.list {}` answers `{ farik: [{ host, shop, category, on, at? }], owner: [{ host, at, request? }], waiting: [{ request, host, url, why, task_id, agent_id, at }] }`: `farik` every entry of Farik's list in its order, `on` whether it is in the approved set and `at` the owner's last turn-off or turn-on, if any; `owner` the approved hosts not on Farik's list, by host; `waiting` by host. `waiting.list` gains a row of kind `site_request` per waiting request, carrying `request`, `host`, `url` and `why`, line "<agent name> asks to read <host>", after the posts outside the plan; the agent's activity line is "Waiting on you: may <agent name> read <host>?" (`crates/store/src/activity.rs`).
- **Today and the agent's page** (Task 0's mockups, on `canvas.json`'s page "Procurement Specialist"). Today lists each waiting request as a row of its own: "<agent name> asks to read <host>" (`siteRequestLine`), the host in bold and in the code face, as stored, in its ASCII form; "For <task> <title>. The task waits until you decide." (`siteRequestTask`); when a label starts with `xn--`, the warning "This name is written in another alphabet. Check it is the site you expect." (`siteRequestScript`); "The page <agent name> wants to read" (`siteRequestPage`) over the address as text, never a link; "Why, in <agent name>'s words" (`siteRequestWhy`) over why in an `untrusted` frame; "<agent name> reads only Farik's approved sites and the sites you allow. Allowing <host> lets it read any page there until you remove it." (`siteRequestWhat`); and under them "Allow" and "Don't allow" (`siteRequestAllow`, `siteRequestDecline`). Each opens the `SiteRequest` dialog, which takes an optional note, "A note for <agent name> (optional)" (`siteRequestNote`), and decides with one button named as the choice: "Allow <agent name> to read <host>?" (`siteAllowTitle`) with "<agent name> can then read any page on <host>, for this task and later ones, until you remove it on <agent name>'s page." (`siteAllowBody`); "Don't allow <agent name> to read <host>?" (`siteDeclineTitle`), the warning again for an `xn--` host, with "<agent name> is told you said no, with your note, and goes on without it." (`siteDeclineBody`); Close decides nothing. The agent's activity line names its first waiting host. The Procurement Specialist's page (`AgentEdit.tsx`) gains "Sites it may read" (`sitesTitle`), between "Skills and connectors" and "What <agent name> may do", with "<agent name> searches the whole web, but opens pages only on these sites. To read another site, <agent name> asks you on Today." (`sitesLead`), in two parts. "Farik's approved sites" (`sitesFarikTitle`), with "Long-established shops Farik checked. Turn one off and <agent name> no longer reads it." (`sitesFarikNote`): a folding row per category in the list's order, closed at first, its label (`siteCategory`) beside, closed, its first three shops' names, and "and <n> more" past three (`sitesMore`), or, open, "<n> shops" and ", <m> turned off" (`sitesCount`); open, each shop's name over its host, "On" or "Off" and a switch, on unless the owner turned it off, a shop that is off saying "You turned it off on <date>." (`sitesTurnedOff`); the switch sends `site_remove` or `site_add` without asking first (one press undoes it). "Sites you allowed" (`sitesOwnTitle`): each host, by host, over "Allowed <day>, when <agent name> asked" (`sitesOwnAsked`) or "Added by you <day>" (`sitesOwnAdded`), <day> being "today", "yesterday" or "on 2 October", and "Remove" (`sitesRemove`), which asks first: "Remove <host>?" (`sitesRemoveTitle`), "<agent name> will no longer read <host>." (`sitesRemoveConfirm`), "To let <agent name> read it again, add it again here, or allow it when <agent name> asks." (`sitesRemoveAgain`), "Keep it" and "Remove". Under them, "Add a site" (`sitesAdd`, O2): a field, "Add", and "A name like shop.com, or the address of any page on it." (`sitesAddHint`); a page's address is kept as its site, and "<agent name> may now read <host>." (`sitesAdded`) says so. The switches, Remove and Add act at once, not through the page's "Save changes". `siteCategory`: `general_marketplace` "Marketplaces", `office_supplies` "Office supplies", `industrial_supplies` "Industrial supplies", `packaging_and_shipping` "Packaging and shipping", `electronic_components` "Electronic components", `computers_and_it` "Computers and IT", `furniture` "Furniture", `food_service` "Restaurant and kitchen supplies", `printing` "Printing", `software` "Software marketplaces". The `en.ts` keys are those in parentheses.
- **The command line.** `farik site list` (Farik's sites with on or off, the owner's, the waiting requests), `farik site approve <n> [--note <text>]`, `farik site decline <n> [--note <text>]`, `farik site add <site>`, `farik site remove <host>` (`crates/cli/src/site.rs`); when a process driving the project ends, each waiting request reads "<task> waits: <agent name> asks to read <host>: farik site approve <n>, or farik site decline <n>" (`crates/cli/src/waiting.rs`), with `--json` a `request` field.
- **The prompt.** `sourcing-a-product` and the role's `system.md` say: you read only Farik's approved sites and the sites the owner allowed, which `farik_read_sites` lists with each shop's category; look there first; search freely with `WebSearch`; find the other sellers, then ask for their sites at once with `farik_request_sites`, each with why, and end your turn; never put the business's details in an address.
- **Tool counts.** The two tools follow `farik_write_evaluation` in `TOOLS`: the read-tier slice goes from `[..30]` to `[..32]`, `daemon/mcp.rs`'s count from 37 to 39; both are left out of `gives_a_session_the_farik_tools_of_its_tiers` (`rules.rs:4919`).

## Farik's approved sites, first version

Forty-six hosts in ten categories, each the shop's primary domain (mostly its United States site). A regional domain (`amazon.co.uk`, `ebay.de`) is a host of its own under the exact-match rule; the launch review decides which regions to add, and the owner may add any meanwhile. Task 2 writes these entries, in this order, with these names.

| `category` | Shops (`host`) |
|---|---|
| `general_marketplace` | Amazon `amazon.com`; eBay `ebay.com`; Walmart `walmart.com`; Costco `costco.com`; Sam's Club `samsclub.com`; Etsy `etsy.com` |
| `office_supplies` | Staples `staples.com`; Office Depot `officedepot.com`; Quill `quill.com` |
| `industrial_supplies` | Grainger `grainger.com`; McMaster-Carr `mcmaster.com`; Fastenal `fastenal.com`; MSC Industrial Supply `mscdirect.com`; Global Industrial `globalindustrial.com`; The Home Depot `homedepot.com`; Lowe's `lowes.com` |
| `packaging_and_shipping` | Uline `uline.com`; Paper Mart `papermart.com`; UPS `ups.com`; FedEx `fedex.com`; USPS Postal Store `store.usps.com` |
| `electronic_components` | DigiKey `digikey.com`; Mouser Electronics `mouser.com`; Arrow Electronics `arrow.com`; Newark `newark.com`; Adafruit `adafruit.com`; SparkFun `sparkfun.com` |
| `computers_and_it` | CDW `cdw.com`; Insight `insight.com`; B&H Photo Video `bhphotovideo.com`; Best Buy `bestbuy.com`; Apple `apple.com`; Dell `dell.com`; Lenovo `lenovo.com` |
| `furniture` | IKEA `ikea.com`; Wayfair `wayfair.com`; National Business Furniture `nationalbusinessfurniture.com` |
| `food_service` | WebstaurantStore `webstaurantstore.com`; KaTom Restaurant Supply `katom.com`; Central Restaurant Products `centralrestaurant.com` |
| `printing` | Vistaprint `vistaprint.com`; MOO `moo.com`; 4imprint `4imprint.com` |
| `software` | AWS Marketplace `aws.amazon.com`; Microsoft AppSource `appsource.microsoft.com`; Atlassian Marketplace `marketplace.atlassian.com` |

Left out: software review sites (G2, Capterra, TrustRadius), which sell nothing and rank vendors partly by paid placement, so software is bought through the official marketplaces above or, by request, the vendor's own site; classifieds between private people (Craigslist, Facebook Marketplace, the latter behind a sign-in), which are not shops; the cross-border marketplaces AliExpress, Temu, Shein and Wish, over regulators' recurring product-safety findings; makers that sell through dealers rather than a shop of their own. Left for the launch review: Alibaba.com, a directory of manufacturers more than a shop, whose standing is a judgment the review makes; USPS's prices (`postcalc.usps.com`, a host of its own) and the other carriers' regional sites.

## Answered by the founder

- **O1. How long an approval lasts** (until the owner removes it, or for the asking task only; recommended: until removed): "Until you remove it". As recommended.
- **O2. May the owner add a site no request named** (recommended: yes): "We will compile a list of approved websites prior to launch and add it as farik approved websites. The user may add more if he chooses to". `site_add`, `farik site add` and "Add a site" stay.
- **O3. Does the role start with any site allowed** (recommended, now superseded: none, leaving marketplace prices to the kit's SerpApi and eBay connectors): "Choose trusted shops across different products categories." Farik's approved sites, above; the planner chose the first version, and the turn-off and the run-time reading are decided above.

## File map

```
docs/design/mockups/{TodaySiteRequest,PhoneSiteRequest,AgentSites,PhoneAgentSites}.dc.html, canvas.json   creates, modifies (Task 0)
crates/core/Cargo.toml, crates/core/src/governor.rs, crates/core/src/governor/sites.rs   modifies, creates (Task 1)
crates/core/src/governor/permissions.rs                       modifies: check_site_urls beside check_urls (Task 1)
crates/roles/roles/procurement_specialist/approved_sites.yaml, docs/schemas/approved-sites.schema.json   creates (Task 2)
crates/roles/src/{sites.rs,lib.rs,generated/mod.rs}            creates, modifies (Task 2)
docs/schemas/{event,command,rpc}.schema.json, crates/protocol/src/{event.rs,command.rs,lib.rs,event/fixtures.rs}   modifies (Task 3)
crates/store/src/migrations/0014_site_requests.sql, crates/store/src/{migrations.rs,projections.rs,sites.rs,waiting.rs,activity.rs,lib.rs}   creates, modifies (Task 3)
crates/runtime/src/tools/sites.rs, crates/runtime/src/tools.rs, crates/runtime/src/daemon/mcp.rs   creates, modifies (Task 4)
crates/runtime/src/orchestrator/session.rs, crates/runtime/src/tools/work.rs   modifies: offered_tools; the transition's refusal (Task 4)
crates/runtime/src/daemon.rs, crates/runtime/src/daemon/hooks.rs   modifies: SessionRegistration.web; judge_call, judge_connector (Task 5)
crates/runtime/src/orchestrator/session.rs                     modifies: the registration's web (Task 5)
crates/runtime/src/orchestrator/{human.rs,messages.rs}, crates/runtime/src/daemon/gates.rs   modifies: the commands, human_message, sites.list (Task 6)
crates/cli/src/{site.rs,main.rs,waiting.rs}, crates/cli/tests/human.rs   creates, modifies, tests (Task 6)
crates/roles/roles/procurement_specialist/{system.md,skills/sourcing-a-product/SKILL.md}, crates/roles/src/lib.rs   modifies (Task 7)
apps/web/src/pages/{Today.tsx,AgentEdit.tsx}, apps/web/src/pages/dialogs/SiteRequest.tsx, apps/web/src/strings/en.ts   modifies, creates (Task 8)
apps/web/src/pages/{Today.test.tsx,sites.test.tsx}, apps/web/src/pages/dialogs/SiteRequest.test.tsx   tests (Task 8)
docs/SPEC.md, docs/design/procurement-specialist.md, docs/plans/project-plan.md   modifies (Task 9)
```

## Interfaces

Consumes: `Role`, `check_urls`, `evaluate_connector_call` (`farik-core`); `load_kit`'s schema check (`farik-roles`); `EventLog`, `EventQuery`, `Projections`, `waiting`, `Waiting` (`farik-store`); `SessionRegistration`, `judge_call`, `judge_connector`, `decide_tool_call`, `human_message`, `offered_tools`, `request_transition` (`farik-runtime`); from step 10b, `Role::ProcurementSpecialist`, its folder, and `TOOLS` after `farik_write_evaluation`.

Produces:

```rust
pub enum WebAccess { Open, ApprovedSites }                        // farik_core::governor::sites
pub fn web_access(role: Role) -> WebAccess;
pub enum SiteFault { NotUrl, NotHttps, HasUserInfo, HasPort, NotADomain, TooLong }
pub fn site_of(address: &str) -> Result<String, SiteFault>;       // the site: one leading `www.` dropped
pub struct SiteRefusal { pub address: String }
pub fn check_site_urls(input: &serde_json::Value, approved: &BTreeSet<String>) -> Result<(), SiteRefusal>;
pub struct FarikSite { pub host: String, pub shop: String, pub category: SiteCategory }   // farik_roles::sites
// SiteCategory: generated from approved-sites.schema.json's category enum, its ten ids
pub enum SitesError { Invalid { detail: String } }                // Display and Error by hand
pub fn parse_farik_sites(yaml: &str) -> Result<Vec<FarikSite>, SitesError>;
pub fn farik_sites() -> &'static [FarikSite];                     // the shipped file, parsed once
pub fn approved_sites(log: &EventLog, farik: &BTreeSet<String>) -> Result<BTreeSet<String>, StoreError>;   // farik_store::sites
pub struct RequestSitesInput { pub sites: Vec<SiteAsk> }           // farik_runtime::tools::sites
pub struct SiteAsk { pub url: String, pub why: String }
// SessionRegistration gains `pub web: WebAccess`; Command gains SiteDecide { request: u64, allow: bool, note: Option<String> },
// SiteAdd { site: String }, SiteRemove { host: String }; WaitingKind gains SiteRequest
```

## Tasks

A test after Task 2 that needs one of Farik's sites takes it from `farik_sites()`, never as a written host, so the launch review edits the YAML alone; the hosts these lines name (`grainger.com`, `mcmaster.com`) stand for such an entry.

### Task 0: Mockups

Files: `TodaySiteRequest.dc.html`, `PhoneSiteRequest.dc.html` (Today's gate: two requests of one task, one an `xn--` name, each with "Allow" and "Don't allow" and the line naming Farik's approved sites; the dialog of each, one with a note), `AgentSites.dc.html`, `PhoneAgentSites.dc.html` (the Procurement Specialist's page, "Sites it may read": Farik's approved sites by category, two categories open, one shop turned off; "Sites you allowed" with three sites, Remove and its question, "Add a site" with a page's address pasted and the site it keeps), `canvas.json`. The founder approves them before Task 8; the approval, with its date and canvas version, is written into this plan's Execution notes in the same commit.

- [ ] `docs(design): mock up asking for a site and the approved sites`

### Task 1: What a site is

Files: `crates/core/Cargo.toml` (`url`), `governor.rs` (`pub mod sites`), `governor/sites.rs`, `permissions.rs`.

- `a_site_is_an_https_named_host`: `https://Shop.Example.com/p?q=1#f`, `https://shop.example.com:443/`, `https://shop.example.com./` and `https://www.Shop.Example.com/` each give `shop.example.com`. RED: no such function.
- `refuses_what_is_not_a_site`: `http://a.com`, `https://a.com:8443/`, `https://u:p@a.com/`, `https://127.0.0.1/`, `https://[::1]/`, `https://localhost/`, `ftp://a.com/`, `a.com` and the empty string are each refused with its `SiteFault`. RED: no such function.
- `an_international_name_is_its_ascii_form`: `https://bücher.example/` gives `xn--bcher-kva.example`. RED: no such function.
- `www_is_the_same_site_and_nothing_else_is`: `https://www.example.com/` and `https://example.com/` both give `example.com`; `shop.example.com`, `example.com.evil.net` and `wwwexample.com` are each their own site; `https://www.com/` gives `www.com`. RED: no such function.
- `holds_every_url_field_to_the_sites`: with `a.com` approved, `{ url: "https://a.com/x" }` and `{ q: { urls: ["https://www.a.com/"] } }` pass; `{ url: "https://b.com/" }`, `{ deep: [{ url: "https://b.com/" }] }`, `{ urls: ["https://a.com/", "https://b.com/"] }`, `{ url: 7 }` and `{ urls: "https://a.com/" }` are refused, each naming the address or the field's JSON. RED: no such function.
- `only_procurement_is_held`: `web_access` is `ApprovedSites` for the Procurement Specialist and `Open` for the eight other roles. RED: no such function.

- [ ] `feat(core): say what an approved site is`

### Task 2: Farik's approved sites

Files: `approved_sites.yaml` (the table above), `approved-sites.schema.json`, `crates/roles/src/sites.rs`, `lib.rs` (`pub mod sites`), `generated/mod.rs` (the schema's types).

- `refuses_a_malformed_site_list`: hand-written lists, each refused naming its entry: a host `https://a.com`, `A.com`, `a.com:443`, `a.com/x`, `10.0.0.1`, `bücher.example`, `www.a.com`, `a.com` twice, an unknown category, an empty shop, a shop with a line break, and an entry with a fourth key; a list of two good entries parses to them in order. RED: no such function.
- `farik_s_approved_sites_are_well_formed`: the shipped file parses (`farik_sites()` does not panic), every host `site_of("https://<host>/")` gives back unchanged, no host twice, every one of the ten categories has at least one shop, and every `shop` is non-empty; the count is not asserted, since the launch review changes it. RED: no such file.

- [ ] `feat(roles): ship Farik's approved sites`

### Task 3: The events, the waiting and the store

Files: the event, command and RPC schemas and their exhaustive matches; the migration and `MIGRATIONS`; `projections.rs`; `sites.rs`; `waiting.rs`; `activity.rs`.

- `round_trips_every_site_event` (`protocol`): each of the four kinds validates against the schema and reads back equal; `EVERY_KIND` counts them. RED: no such kinds.
- `the_approved_sites_are_the_log_s`: with Farik's `{f.com}`, an empty log gives `{f.com}`; approve `a.com`, approve `b.com`, remove `a.com`, remove `f.com` gives `{b.com}`; approving `a.com` and `f.com` again gives `{a.com, b.com, f.com}`. RED: no such function.
- `a_new_farik_site_arrives_and_a_turn_off_stays`: one log (remove `f.com`, approve `o.com`) folded with the next release's list `{f.com, g.com, h.com}` gives `{g.com, h.com, o.com}`, the new `h.com` open and the turned-off `f.com` still off; with a list `{f.com, h.com}` that dropped `g.com`, it gives `{h.com, o.com}`. RED: no such function.
- `a_site_request_makes_its_task_wait`: `site.requested` raises the task's `open_sites` and marks it waiting on the human; a `site.approved` with its `request` lowers it, as a `site.declined` does; an added site with no `request` lowers nothing. RED: no column.
- `waiting_lists_each_site_request`: one undecided request gives a `site_request` row with `request`, `host`, `url` and `why` and the line "Kai asks to read shop.example", after the posts outside the plan; a decided one gives none; the agent's activity line is "Waiting on you: may Kai read shop.example?". RED: no such kind.

- [ ] `feat(store): record sites asked for, approved and removed`

### Task 4: The agent's two tools

Files: `tools/sites.rs`, `tools.rs` (two descriptors after `farik_write_evaluation`, the slice `[..32]`, their arms in `call_tool`), `daemon/mcp.rs` (39), `offered_tools`, `tools/work.rs`.

- `asks_for_each_new_site`: four entries, one owner-approved, one `https://www.grainger.com/` (Farik's), two new, record two `site.requested` with the task, agent and session, answer `allowed`, `allowed`, `asked`, `asked` with their numbers, and end with `next`. RED: no such tool.
- `does_not_ask_twice`: an entry for a host already waiting for this task is answered `waiting` and nothing is recorded. RED: no such tool.
- `refuses_a_bad_site_or_why`: `http://a.com`, an IP address, an empty `why`, a why of 301 characters and one with a line break are refused with their codes, and nothing is recorded; eleven entries are refused. RED: no such tool.
- `caps_the_requests_waiting`: with 19 waiting, three new entries record one and answer `full` twice; with 20 waiting the call is `too_many_site_requests`. RED: no such tool.
- `reads_where_it_may_read`: `farik_read_sites` gives Farik's hosts with shop and category, less one turned off, and the owner's with neither, this task's waiting hosts, and its declined ones with their notes. RED: no such tool.
- `offers_the_site_tools_to_a_held_role`: a procurement `implement` session is offered both, its chat `farik_read_sites` alone, a Finance Specialist's and a Marketing Specialist's sessions neither. RED: no such tools.
- `cannot_hand_in_while_a_site_waits`: with a request waiting, `farik_request_transition` to `verifying` is `site_request_waiting: shop.example waits for the owner; end your turn` and records nothing; once it is decided, the move goes through. RED: no such refusal.

- [ ] `feat(runtime): let the Procurement Specialist ask for a site`

### Task 5: The hook holds the role to its sites

Files: `daemon.rs` (`SessionRegistration.web`), `orchestrator/session.rs` (sets it where the registration is made) and the tests' registrations, `daemon/hooks.rs`.

- `a_farik_site_is_open_from_the_start`: on a log with no site event, a procurement session's `WebFetch` of `https://www.mcmaster.com/` is allowed and of `https://shop.example/` denied `site_not_approved`. RED: every `WebFetch` passes on `network`.
- `procurement_fetches_only_approved_sites`: with `shop.example` approved, a procurement session's `WebFetch` of `https://www.shop.example/prices` is allowed and of `https://other.example/` denied `site_not_approved`, the session not stopped; after `site.removed`, the next `WebFetch` of `shop.example` is denied. RED: as above.
- `a_conversation_is_held_too`: the role's `conversation` session (a mention in the channel, holding `network`) is denied `site_not_approved` for an unapproved site. RED: as above.
- `other_roles_fetch_as_before` (a guard): a Marketing Specialist's `WebFetch` of any `https` address is allowed.
- `procurement_searches_freely` (a guard): a procurement session's `WebSearch` is allowed, approved sites or not.
- `a_connector_s_addresses_are_held_too`: a user's server given to a Procurement Specialist, its tool tagged `network`, is denied `site_not_approved` for `{ url: "https://other.example/" }` and allowed for an approved one; the same call of a Developer's is allowed; an `external_effect` tool's call to an unapproved address is refused `site_not_approved` without asking, whether or not a grant matches it. RED: connectors are judged by tag alone.

- [ ] `feat(runtime): hold the Procurement Specialist's web reading to approved sites`

### Task 6: The owner decides

Files: `human.rs` (`site_decide`, `site_add`, `site_remove`), `messages.rs` (`human_message`), `daemon/gates.rs` (`sites.list`, the `site_request` rows), `crates/cli/src/{site.rs,main.rs,waiting.rs}`, `crates/cli/tests/human.rs`.

- `allowing_a_request_approves_its_site`: `site_decide { allow: true }` records `site.approved { host, request }` on the request's task, with no agent or session, and the host is in `approved_sites`. RED: no such command.
- `allowing_settles_every_request_for_the_site`: two tasks' requests for `shop.example`; allowing one records a second `site.approved` for the other, and neither task waits. RED: no such command.
- `a_request_is_decided_once`: a second decision is `site_request_decided`, a seq that is no request `unknown_site_request`, and two decisions at once let exactly one through. RED: no such command.
- `the_owner_adds_a_site_unasked`: `site_add { site: "https://www.shop.example/x" }` records `site.approved { host: "shop.example" }` with no request and no task, and a procurement `WebFetch` of it is then allowed; again, `site_already_allowed`; `site_add` of an on Farik site is `site_already_allowed`; `site_add { site: "http://a.com" }` is `site_invalid`; `site_remove` records `site.removed`; again, `site_not_allowed`. RED: no such commands.
- `the_owner_turns_off_a_farik_site`: `site_remove { host: "grainger.com" }` records `site.removed { host: "grainger.com" }`; a procurement `WebFetch` of it is then denied `site_not_approved`; `sites.list` shows it under `farik` with `on: false` and the turn-off's `at`; `site_add { site: "grainger.com" }` turns it back on. RED: no such command.
- `the_next_session_is_told`: after one site allowed with a note and one declined without, the task's next session's human message holds "The owner allowed you to read shop.example. The owner adds: <note>" and "The owner did not allow other.example."; a `site.declined` whose envelope names an agent is not told. RED: no such lines.
- `lists_the_sites`: `sites.list` answers Farik's entries in order with `shop`, `category` and `on`, the owner's hosts, and the waiting requests with their fields. RED: no such query.
- `farik_site_lists_and_decides` (`cli/tests/human.rs`): `farik site approve <n> --note ok` and `farik site decline <n>` record their decisions; `farik site remove grainger.com` turns it off; `farik site list` prints Farik's sites with on or off, the owner's, and the waiting requests; a process ending prints the waiting line with both commands. RED: no such commands.

- [ ] `feat(runtime): let the owner allow, refuse, add and remove sites`

### Task 7: The prompt says how to ask

Files: the role's `system.md` and `sourcing-a-product/SKILL.md`; `crates/roles/src/lib.rs`.

- `sourcing_a_product_says_how_to_ask_for_a_site`: the skill contains "`farik_read_sites`", "`farik_request_sites`", "end your turn" and "never put the business's details in an address", and the prompt "Farik's approved sites and the sites the owner allowed"; `kit_skills_name_only_tools_farik_lists` passes. RED: neither says it.

- [ ] `feat(roles): tell the Procurement Specialist how to ask for a site`

### Task 8: Today and the agent's page

Files: `Today.tsx` (the `site_request` kind), `dialogs/SiteRequest.tsx`, `AgentEdit.tsx` ("Sites it may read"), `en.ts`, their tests; as the approved mockups.

- `today_lists_a_site_request` (`Today.test.tsx`): a `site_request` row shows the host in bold, the address as text with no link, why in an `untrusted` frame, and, for an `xn--` host, `siteRequestScript`. RED: the kind is filtered out.
- `allowing_and_refusing_a_site` (`SiteRequest.test.tsx`): the row's "Allow" opens the dialog, whose "Allow" sends `site_decide { request, allow: true }`; "Don't allow" with a note sends `allow: false` and the note; Close sends nothing. RED: no dialog.
- `the_agent_page_lists_its_sites` (`sites.test.tsx`): a Procurement Specialist's page lists `sites.list`'s Farik sites by category with their names and switches, and the owner's hosts; a closed category names its first three shops; turning a Farik site off sends `site_remove`, on `site_add`, neither asking; "Remove" asks, then sends `site_remove`; "Add a site" sends `site_add`; a Developer's page has no such section. RED: no section.

- [ ] `feat(web): ask the owner about sites and list the approved ones`

### Task 9: Spec and plan

`docs/SPEC.md`: 5.6 (a role held to approved sites; `WebFetch` and a connector's `url` fields judged against them; `WebSearch` unchanged; the sentence on the order of a connector call in the hook gains the sites after the preview's `url`s); 5.7 (a site request waits as a marketing plan does; `open_sites`; the decisions told); 6.10 (the role reads only Farik's approved sites and the sites the owner allowed; it starts with Farik's; its two tools); 8.2 (`SessionRegistration.web`); 8.5 (the four events); 8.6 (what the list stops; Farik's approved sites, a public list shipped in the role's folder, not a secret, read at run time so a release's additions reach every team on upgrade while the owner's turn-offs and approvals stay; the residuals: data can still reach an approved site in an address, and Farik's list makes those some forty-six shops Farik chose, not the owner, whose marketplaces carry sellers' pages, untrusted as before; a connector's address in a field not named `url` or `urls`; a `WebSearch` query leaves the computer for the search service; what a session read before a removal stays in it; an approved name may later point elsewhere); F9 (the agent page's two lists, Farik's with a switch per shop, the owner's with Remove and "Add a site"); the revision line. `docs/design/procurement-specialist.md`: line 38's tiers, with the approved sites and Farik's list, and the tools table's two rows. `docs/plans/project-plan.md`: row 10b2, what was executed.

- [ ] `docs(spec): record the approved sites`

## Verification

```
cargo xtask check
# expected: xtask check: ok (with pnpm check)
```

Then, in the web app, by the founder (step 10b's run, moved here): add a Procurement Specialist to a team of six and see "Sites it may read" list Farik's approved sites; file "Compare three email-sending services for about 3,000 emails a month"; see it search, then ask on Today for the services' sites; allow two and decline one with a note; see the evaluation and the register written in `.farik/local/procurement/`, the Product Manager's review, and the acceptance with nothing to integrate, with `git status` showing nothing new; then file "Price 500 corrugated shipping boxes, 12 by 9 by 6 inches", see it read a shop on Farik's list without asking, turn that shop off on the agent's page, and see the log (`farik log`) record the next `WebFetch` of it as `tool.denied` with `site_not_approved`.

## Execution notes

None yet.
