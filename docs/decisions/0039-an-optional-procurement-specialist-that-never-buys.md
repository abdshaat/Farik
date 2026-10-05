# 0039. An optional Procurement Specialist that never buys

Date: 2026-10-05
Status: proposed. The founder asked for the role on 2026-10-05 ("Add and plan a procurement agent and plan all its tools and connectors as well as skills"), and the same day set its research focus and the data pipeline request ("He may request a data pipeline from the pm who can decide whether the data pipeline is necessary or whether this decision must be escalated to the owner"); the rest of the shape below is the planner's, and the ADR is accepted when the founder answers O1 to O6 at the end of `docs/design/procurement-specialist.md`.

## Context

On 2026-10-05 the founder asked for a procurement agent in phase 7, with its tools, connectors and skills planned like every other role's kit.

A team that builds a software product buys things all the time, and nobody on it owns the buying. It buys an email-sending service, a database host, an error tracker, a domain, a design tool, a higher plan of the AI account itself. Today the Architect reviews open-source dependencies and their licences (spec 6.3), and the Finance Specialist records what was spent once it was spent (spec 6.6). Nobody compares the paid options before the money goes, reads their terms, or notices that a yearly plan renews next week.

Four facts constrained the design:
- A purchase moves the user's money. A vendor's page is untrusted content (spec 8.6), and a page can be written to steer an agent towards one product, one plan, or one "accept" button.
- Farik holds no card, and should not. A payment connector would put a card where a prompt can reach it, and a non-technical user approving a stream of purchase prompts is the approval fatigue spec 5.6 avoids.
- Prices, quotes and contracts are confidential, and the repository may be public (ADR 0019 found this one is).
- A team holds at most seven agents (D18), and the team builder suggests six.

The options for what the agent may do at the point of buying were these:
- **Buy through a payment or procurement platform, each purchase approved.** Ramp's hosted server has a checkout, Porkbun's buys domains. Every call would ask (a tool that pays has no allowance, spec 6.7), but the agent would still choose the vendor, the plan and the amount, and the approval would be a click on its choice.
- **Never buy; ask the human to buy.** The agent researches, compares, recommends and files a purchase request with the price, the plan, the link and the evaluation behind it; the human buys at the vendor and says so in Farik, with what was paid. This is the chosen way.

The options for where its work lives were these:
- **In the repository, under the team's `document_paths`, as the Marketing Specialist's does.** Simple, but a public repository would publish negotiated prices and contract terms.
- **A private folder, `.farik/local/procurement/`, under the same rules as the Finance Specialist's folder.** Never committed; its session runs in the folder; its register is a workbook through the Finance Specialist's sheet tools. This is the chosen place.

The options for renewals were these:
- **A session that checks the register every day.** It spends the model's tokens to read a date.
- **A tick with no model.** Farik reads the register's dates once a day, as the receipts sweep and the DevOps watch do, and tells the human when a renewal's decision date nears. This is the chosen way.

## Decision

Add a ninth role, the Procurement Specialist (`procurement_specialist`). It is optional: the team builder offers it and does not suggest it. It has the `read` and `network` tiers, the Product Manager is its reviewer, and its default model is the Marketing Specialist's.

It finds, compares and recommends the paid services the product needs, checks their terms and their security, keeps the register of what the team pays for, and reviews each renewal before its decision date. It never pays, signs up, starts a trial that takes a card, accepts terms, signs, cancels, or writes to a vendor. It files a purchase request (`farik_request_purchase`), which waits on Today until the human marks it bought, with what was paid, or not bought.

Its work lives in `.farik/local/procurement/`, never committed: `vendors.xlsx`, the register, written through `farik_write_sheet`, and `evaluations/<name>.md`, written through `farik_write_evaluation`. Its tasks follow the Finance Specialist's folder rules (spec 6.6): the session runs in the folder, there is no branch and nothing to integrate, and one piece of procurement work touches the folder at a time. Phase 7 step 09 builds those rules keyed by role, so this role adds a second folder rather than a second set of exceptions. The Finance Specialist may read the register; nothing else outside the role may.

Its first job is research, prices and providers. When a source it lacks would change a recommendation, it asks the Product Manager for a data pipeline (`farik_request_data_pipeline`). The Product Manager decides whether it is needed, approving, declining or escalating it to the owner; the governor refuses the Product Manager's approval of one that costs money, needs an account, or sends the project's data out, so those always reach the owner. An approved pipeline files an ordinary request for the team; approval itself connects, pays for and builds nothing.

Once a day, while a process drives the project, a renewal tick with no model reads the register's renewal and notice dates and records `renewal.flagged` when a renewal's decision date is 14 days off or nearer. Today shows it, with "Ask for a review", which files an ordinary request, and "Dismiss".

Its kit ships eight skills and three connectors, each read only: Farik's own currency-rate server over Frankfurter (`farik connector fx`, ADR 0038's pattern, no account), AWS's official pricing server (a key limited to the pricing read actions), and Brex's official server (signed in by route 1, read-only scopes, its people, card and bank tools `denied`). Ramp, Porkbun, DocuSign, Vercel's domains, Zylo, Vanta, Drata, SafeBase, G2, Cloudflare, Vantage and a mail connector are rejected for now; the design says why for each.

The role is phase 7 steps 10b and 10c, after the Finance Specialist's kit (step 10), whose folder rules and sheet tools it reuses; its kit is step 10d; data pipeline requests are step 10e, with a candidate list of sources the design keeps. Step 13, the kit check, gains an eighth task.

## Consequences

Easier:
- Someone on the team compares the paid options before the money goes, in one currency, over 12 and 36 months, and says what the terms and the exit cost.
- No prompt, page or vendor can spend the user's money through Farik: there is no tool that can.
- A renewal is seen before its notice period ends, at no model cost.
- The Finance Specialist forecasts from a register someone keeps, and a purchase the human marks bought is the record of it.

Harder:
- The human still does every purchase by hand. A step Farik could have automated stays a step, by design.
- The register is a workbook the user may edit by hand, and the renewal tick reads its dates. A date it cannot read is skipped and said once on Today, not guessed.
- A ninth role; a team of the six suggested agents and one optional role is the cap of seven, so adding this role beside the Finance Specialist or the DevOps Engineer means unticking a suggested one.
- The role leans on step 09's private-folder rules. Step 09 must build them for more than one role, which its plan now has to say.
- AWS's pricing server asks a non-technical user for an access key. The setup copy has to make a key limited to reading prices possible to create, and the connector stays optional.
- A data pipeline request is one more thing that can wait on the Product Manager and the owner, and one more decision session that costs model time.
- Brex's tool list "is subject to change"; the live pin test catches it, and a pin update re-reviews every tag.
