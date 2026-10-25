# 0010. Prospects are public signals, and Praeco never contacts them

Date: 2026-10-24
Status: proposed

## Context

The founder wants the agent to "collect potential customers". The usual tools for that (email finders, data brokers, automated outreach) collect personal data under rules (GDPR, CAN-SPAM, the platforms' terms) that a small company rarely has the basis for, and automated outreach is the fastest way to get a domain blocked and a brand known for spam. Catervas's Procurement Specialist set the pattern for contacting outsiders: the agent drafts, the person sends (Catervas ADR 0039, its step 10f).

## Decision

- A prospect is a **signal** (a public post showing the need: platform, URL, the public handle as shown, the time it was posted, a quote of at most 280 characters, the need in one sentence, the criteria it meets) or a **company** (name, domain, why it fits, the source). Nothing else about a person is stored (SPEC 5.9).
- `record_prospect` refuses a quote with an email address or phone number in it, and there is no field for contact details. Praeco has no enrichment and no email-finding connector.
- The fit score is `@praeco/core` `scoreProspect`, computed from the criteria the agent shows evidence for and the weights in the workspace's profile of a good fit; the model never writes the score.
- Unsaved prospects are deleted 90 days after they were found; a deletion request deletes the prospect and suppresses its handle for the workspace; events carry a prospect's id only.
- **Praeco never contacts a prospect.** No tool that sends a message to anyone outside the workspace exists; the `denied` tier is for such tools and none is offered. Replies are drafts a member copies and posts themselves, and "I contacted them" is their record.

## Consequences

The prospect inbox is something a company can defend to its own customers, and the drafts are better for being sent by a person who read the thread.

What cuts against it: no automated outreach is a real limit to how many leads Praeco "collects"; the measure in M1 is contacted prospects, not stored ones, which keeps the product honest about that.
