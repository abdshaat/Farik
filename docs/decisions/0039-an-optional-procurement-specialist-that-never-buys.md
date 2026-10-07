# 0039. An optional Procurement Specialist that never buys

Date: 2026-10-05
Status: accepted. The founder asked for the role on 2026-10-05 ("Add and plan a procurement agent and plan all its tools and connectors as well as skills"), set its research focus and the data pipeline request the same day, and answered the design's O1 to O6 ("the agent never directly buy[s], he just compiles list of sellers, look for price, contact manufacturer or sellers. Get prices. And set up a purchase order but the final decision is the founder['s]"; "all the data are and spreadsheets are to stay local"; "any product whether technical or non technical"; "after the finance specialist and before devops"; "the PM must escalate any process that cost money").
Amended 2026-10-05 by ADR 0041: a team the user runs on auto sends the agent's messages to sellers as drafted, within the daily cap, and approves escalated data pipelines; purchase orders still wait for the founder. Phase numbers after the web launch moved up by one (ADR 0040).
Amended 2026-10-07 by the founder's answers to phase 7 step 10b's readiness review: the Procurement Specialist fetches only the sites the owner approved ("Restrict its web access"), in a new step 10b2; and it is not offered `farik_read_costs` ("No"). The same day the founder answered step 10b2's three questions: an approval lasts "Until you remove it", the owner may add sites, and the role starts with Farik's approved sites, trusted shops Farik ships ("Choose trusted shops across different products categories"). See "Amendment of 2026-10-07" at the end.
Amended 2026-10-07 by the founder's answers to phase 7 step 10c's readiness review, in conversation: a purchase order's seller page must be on an approved site ("Yes, approved sites only"); an order left waiting closes by itself ("Expire after 30 days"); and "Mark received" offers to ask the agent to update the register ("Ask the agent, ticked"). Later the same day: "Slight change. The procurement agent only researches and follows up and store the status of an order. He may not execute any orders by himself", clarified as "Suggest, then track" and "The agent, from follow-ups": the founder marks an order placed and received, and the agent records the order's status from its follow-ups. See "Amendment of 2026-10-07: suggest, then track" at the end.

## Context

A team buys things, and nobody on a Farik team owns buying. The Architect reviews open-source dependencies (spec 6.3); the Finance Specialist records money after it is spent (spec 6.6). Nobody finds sellers, compares prices, asks for a quote, or prepares an order. The founder wants an agent that does, for any product: a software subscription, stock to resell, a used car, a baby car mirror. The same day the founder set the direction of ADR 0040 (proposed), Farik for businesses that do not ship software, for which buying is daily work.

Four facts constrained the design:
- A purchase moves the user's money. A seller's page or reply is untrusted content (spec 8.6), and can be written to steer an agent towards one product, one seller or one "accept" button.
- Farik holds no card and should not. A payment connector would put a card where a prompt can reach it.
- Writing to a seller is an act in the world: a message cannot be unsent, and a careless one commits the business or spams a stranger. Spec 6.7 already says a tool that sends always asks.
- Prices, quotes, orders and correspondence are confidential, and the repository may be public.

The options at the point of buying were these:
- **Buy through a platform, each purchase approved.** The agent would still choose the seller and the amount, and the approval would be a click on its choice.
- **Never buy; prepare a purchase order the founder decides.** The agent finds sellers, gets prices and quotes, and drafts an order; the founder approves or rejects it and places it, or has Farik send it to the seller. This is the chosen way.

The options for contacting sellers were these:
- **A mail connector in the kit, its send tool `external_effect`.** The agent would choose when to call it and the approval would arrive mid-session, about text the founder had not seen in context.
- **The founder's mail program, drafts only.** Gmail's official server drafts but cannot send; it needs a Google client of the user's own, and Google's route waits until after the launch.
- **A procurement mailbox Farik sends from, each message sent by the founder's press.** The agent drafts; the founder reads, may edit, and sends from Today; Farik reads only the replies. This is the chosen way.

The options for where its work lives were the repository under `document_paths`, or a private folder under the Finance Specialist's rules; the founder chose local.

## Decision

Add a ninth role, the Procurement Specialist (`procurement_specialist`), optional, not suggested, with the `read` and `network` tiers, the Product Manager as its reviewer, and the Marketing Specialist's default model. It researches any product or service: it lists sellers and makers, finds their prices, asks them for quotes, checks the sellers and the products (recalls, safety standards, a used car's VIN), compares offers in one currency with landed cost, keeps the register of sellers and subscriptions, and reviews renewals.

It never pays, bids, checks out, signs up, accepts terms or signs. It sets up a purchase order (`farik_draft_purchase_order`, a workbook in its folder) that the founder approves, rejecting it or placing it themselves, or having Farik send it to the seller; later the founder marks it received with what was paid.

It writes to sellers and makers only through drafts (`farik_draft_seller_message`) that the founder reads, may edit, and sends from Today, from a procurement mailbox of the user's own, at most 50 a day, each signed with an AI-assistance line by default. Farik reads only replies to what it sent, without marking them read, and hands them to the agent as untrusted content.

Everything it keeps is in `.farik/local/procurement/`, never committed, under the folder rules step 09 builds for the Finance Specialist, keyed by role. The Finance Specialist may read the register.

When a source it lacks would change a recommendation, it asks the Product Manager for a data pipeline. The Product Manager decides whether it is needed; the governor refuses the Product Manager's approval of any pipeline that costs money, which goes to the owner. Approval files an ordinary request; it connects, pays for and builds nothing.

A renewal tick with no model reads the register once a day and puts each renewal whose decision date is 14 days off or nearer on Today.

Its kit ships fourteen skills and seven connectors, all read-only but one: Farik's own `fx` (exchange rates), `recalls` (US product and vehicle safety) and `ebay` (eBay listings), and the official Exa (web search), SerpApi (shopping prices, its search counted against an allowance), Brex (spend with a seller, read-only) and AWS Pricing (for software teams). Amazon's buyer API, eBay's official server, supplier directories, Ramp, Porkbun, DocuSign and the rest are rejected for now; the design says why for each and keeps a list of candidates a data pipeline may bring.

It is phase 7 steps 10b (the role and its folder), 10c (purchase orders and renewals), 10d (the kit), 10e (data pipeline requests), 10f (contacting sellers) and 10g (Farik's `recalls` and `ebay` servers), after the Finance Specialist's kit and before the DevOps Engineer. Step 13, the kit check, gains an eighth task.

## Consequences

Easier:
- Someone on the team finds the sellers, gets real quotes and compares them before money goes, for any product.
- No prompt, page or reply can spend the user's money or send a message through Farik: there is no tool that can, and every send is the founder's press.
- A recalled product or a car with an open recall is seen before it is bought.
- A renewal is seen before its notice period ends, at no model cost.

Harder:
- The founder still places or approves every order and sends every message. That is the design.
- Step 10f brings the first mailbox into Farik (IMAP, SMTP and three new crates), before the receipts intake of phase 13, which then reuses it.
- A team of the six suggested agents and one optional role is the cap of seven.
- The role leans on step 09's private-folder rules, which must be built for more than one role.
- SerpApi spends the user's searches; its allowance asks the founder past 50 a sprint.
- AWS Pricing needs the program `uv` and a narrow AWS key; Brex's and Exa's tool lists may change, which the live pin test catches.
- `recalls` covers the United States only until a pipeline brings the EU's or Canada's lists.

## Amendment of 2026-10-07

The role reads sellers' pages, which are untrusted (spec 8.6), while it holds quotes, prices and the register, and its `network` tier let it fetch any address. A page could steer it to fetch an address of the page's choosing with the business's details in it. Asked at step 10b's readiness review whether to accept and record that risk, "or restrict its web access (it may only browse addresses you approve)?", the founder answered "Restrict its web access". Asked whether the role should read the team's AI costs, the founder answered "No".

Decision:
- **It fetches only Farik's approved sites and the sites the owner approved.** A site is a host reached over `https`, matched exactly, a host and its `www.` twin being one site, never its other subdomains and never an IP address. The agent asks with `farik_request_sites`, each site with the reason, and its task waits, as for a marketing plan (ADR 0042), until the owner allows or refuses each on Today. Asked how long an approval lasts, the founder answered "Until you remove it". Asked whether the owner may add a site the agent never asked for, the founder answered "We will compile a list of approved websites prior to launch and add it as farik approved websites. The user may add more if he chooses to": the owner adds a site, and removes one, on the agent's page. Approvals are kept in this computer's event log, per team, never in the committed team file, which a pull could change.
- **It starts with Farik's approved sites.** Asked whether the role starts with any site allowed, the founder answered "Choose trusted shops across different products categories." Farik ships a list of long-established shops, by their official domains, across the categories a small business buys in: marketplaces, office, industrial and food-service supplies, packaging and shipping, electronic components, computers, furniture, printing and software marketplaces. Step 10b2 holds the first version, compiled on 2026-10-07; phase 11 reviews and finalizes it before the launch. It is public data in the role's folder, not a credential, so ADR 0044 does not reach it. It is read at run time, never copied into the log: a site a release adds is open on upgrade, and one a release drops closes unless the owner allowed it. The owner may turn any of Farik's sites off for the team, a removal kept in the log and shown on the agent's page, which outlasts upgrades.
- **The hook holds the fetch, not the search.** `WebFetch`, and every `url` and `urls` field of a connector's call, must name an approved site. `WebSearch` stays open: its query goes only to the search service Claude Code uses, through the model provider, which already sees the whole session, and never to an address a page chooses.
- **`auto` does not approve a site.** Under ADR 0041's `auto` a request still waits for the owner: the list is a limit the owner sets, as the spending limits are, and under `auto` the limits are the only guard against a steered agent.
- **No `farik_read_costs`.** The team's AI costs stay the Finance Specialist's (spec 6.6); spec 6.10's line giving the role the tool is withdrawn by step 10b.
- **Step 10b2**, after 10b and before 10c, builds it; the kit (10d) and the role's first live run come after it.

Consequences:
- Easier: a seller's page can no longer have the agent send the business's details to an address the page names. The agent reads well-known shops from its first task, without the owner answering a request for each.
- Harder: the owner answers a request before the agent reads a new seller's site, under `auto` too. Some paths stay open, and spec 8.6 records them: data can still reach an approved site inside an address; a connector's address in a field not named `url` or `urls` is not judged; a search query still leaves the computer for the search service; and what a session read before a site was removed stays in that session. Farik vouches for the shops on its list, which the owner did not choose: data the agent puts in an address can reach any of them, and a marketplace on it carries many sellers' pages, each untrusted as before; the list is reviewed before the launch and kept current by Farik's releases.

## Amendment of 2026-10-07: suggest, then track

Step 10c's readiness review asked the founder three questions. Must a purchase order's seller page be on an approved site? "Yes, approved sites only." What ends an approved order never placed, or one whose task was cancelled? "Expire after 30 days." Who updates the register after "Mark received"? "Ask the agent, ticked." Later that day the founder narrowed the role: "Slight change. The procurement agent only researches and follows up and store the status of an order. He may not execute any orders by himself"; asked to clarify, "Suggest, then track" (it prepares a suggested order, the founder approves it, places it and pays for it, and the agent then follows up and keeps the order's status) and "The agent, from follow-ups" (the agent records the statuses it learns from its follow-ups, the founder can correct any, and only the founder marks an order placed or received).

Decision:
- **The agent suggests and tracks; it never executes.** It drafts the order (`farik_draft_purchase_order`); it never places, pays for, confirms or cancels one, and no Farik tool records any of those steps or a payment. The founder approves or rejects the order, places and pays for it themselves, marks it placed on the agent's page (with what they paid, if they know), and marks it received, or closes it when it will not come. Under `auto` all of this stays the founder's.
- **The agent records the follow-up status.** From placing to receiving, the agent records what it learns from its follow-ups with `farik_update_purchase_order`: being prepared, shipped, delayed (with the reason and the expected day) or a problem. Each status is recorded in the agent's name, so it never counts as the founder's; the founder can correct any status. Following up is reading what the agent may reach, the seller's pages on approved sites and, from step 10f, the replies to messages the founder sends; the agent sends nothing. A follow-up is a task: "Mark placed" offers to ask the agent to follow up, ticked, as a request the founder reads first.
- **The seller's page is on an approved site.** `farik_draft_purchase_order` refuses a page on any other site, and the agent asks for the site first.
- **An order left waiting closes by itself.** An order not decided within 30 days of its drafting, or approved and not marked placed within 30 days of its approval, expires. A placed order never does: the founder has paid, so it shows as overdue until the founder marks it received or closes it.
- **The register stays the agent's.** "Mark received" offers "Ask <agent> to update the register", ticked, filing a request whose words the founder reads and may edit.

Consequences:
- Easier: the founder sees every order from suggestion to arrival in one place, with what the agent learned, without the agent ever holding the money or the order.
- Harder: the founder records two steps per order, placed and received; a follow-up costs a task each time, and the agent's statuses are only as good as what the seller's pages say, which is why the founder can correct them.
