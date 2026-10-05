# 0042. The Marketing Specialist owns the brand and runs a marketing plan the owner approves

Date: 2026-10-05
Status: accepted (the founder, 2026-10-05: "The marketing agent will own the brand kit, the marketing plan, the brand persona, and the social media presence. He will be able to conduct marketing research to determine the best marketing plan. He may be assigned a marketing budget to spend according to a pre determined plan that is approved by the owner. Please ensure that the marketing agent will be able to have social media write access for instagram, x, and google ads"; and the same day, four answers: posts in an approved plan go out without asking, shown ahead on Today with a Stop; Farik's own Google Ads connection, with Google's sign-in pulled forward; Instagram and X through Buffer; a hard stop at the budget). Amends spec 6.5 ("Cannot … publish anywhere") and 6.7 (a tool that posts always asks), ADR 0035 (Google deferred until after the launch), and ADR 0041 (what auto changes).

## Context

Until now the Marketing Specialist wrote documents and, from phase 7 step 08, made images and clips within an allowance; it never published, and spec 6.7 gave every tool that posts, sends or pays no allowance, so each one asked. The founder wants the role to own the business's marketing: its brand kit and brand persona, its marketing plan, built from research, and its social media presence, with write access to Instagram, X and Google Ads, and to spend a marketing budget by a plan the owner approved.

What was probed on 2026-10-05:
- **Instagram and X posting.** Buffer's official server (step 08b) signs in by route 1 and posts to Instagram, X and nine other networks. X's own hosted server (`https://api.x.com/mcp`) offers no `tweet.write` scope and no client registration, and X now charges per post; Meta publishes no server for organic Instagram posts.
- **Paid Instagram ads.** Meta's official Ads server (`https://mcp.facebook.com/ads`) has read and write tools, but its registration answers "Dynamic registration is not available for this client": Meta admits only listed clients.
- **Google Ads.** Google's official Ads MCP server is read-only (three tools). Writing needs the Google Ads API itself with a Google sign-in; since 2026-09-09 the API needs no developer token, its access following the Google Cloud project. Farik's Google sign-in was deferred until after the launch (ADR 0035's amendment), with its design already decided.
- **Money.** A post through Buffer costs nothing; an ad spends the business's money at Google, which keeps spending while Farik is not running.

The options for how the owner's approval reaches each act:
- **Every post and every ad change asks** (today's rule). Safe, but it makes the owner the scheduler of a social presence the founder wants the role to own.
- **The owner approves a plan, and the plan approves the acts in it.** The pattern of a sprint's start approving its deploys (spec 6.9). This is the chosen way.

## Decision

**The role owns four things**, kept as documents in the project, under `docs/marketing/` (the business's workspace, for a business that is not software):
- the brand kit, `docs/marketing/brand/brand-kit.md` and its files under `docs/marketing/brand/assets/`: the name, the promise, the audience, the colours, the type, the logo and how it is used, the picture style and the voice;
- the brand persona, `docs/marketing/brand/persona.md`: the character the brand speaks as on social media;
- the marketing plan, `docs/marketing/plans/<plan id>.md`, with the research behind it under `docs/marketing/research/`;
- the social presence: the posts on the business's channels.

While the team has an active Marketing Specialist, a task of any other role may not name `docs/marketing/**` in its allowed paths (a Definition of Ready rule); others read them, and the UI/UX Designer takes the project's colours and voice from the brand kit.

**A marketing plan is the owner's decision.** The Marketing Specialist proposes one with a Farik tool, as structured data and its text: its dates (at most 92 days), its budget in one currency and per channel, its ad campaigns (each with its budget and dates), its post slots (channel, day and topic) and how success is measured. The owner approves it or sends it back on Today, and may end it at any time; one plan is active at a time, and a newly approved one replaces the active one from its start. Approval is the owner's alone, under `ask` and `auto` alike (ADR 0041), as a purchase order is (ADR 0039). Events: `marketing_plan.proposed`, `approved`, `returned` and `ended`.

**A post in the approved plan goes out without asking.** The agent never calls Buffer's write tools (they become `denied` in its kit); it calls Farik's `farik_schedule_post`, and Farik makes the call to Buffer with the user's connection. A post that fills an unused slot of the active plan is scheduled at least three hours ahead and shown on Today with its text and pictures and a Stop button until Farik hands it to Buffer, one hour before its time; Stop also takes it back from Buffer while it has not gone out. A post outside the plan asks the owner, or, under `auto`, goes out as ADR 0041 lets any outward act. Events: `social_post.scheduled`, `requested`, `sent`, `stopped`, `missed` and `failed`.

**Google Ads is Farik's own connection** (ADR 0038), signed in with Google through Farik's own Google app, pulled forward from after the launch to phase 7 for this one scope (`https://www.googleapis.com/auth/adwords`), with ADR 0035's decided Google design otherwise unchanged. Its read tools are `network`. Its write tools (create a search campaign, its ad groups, keywords and ads, change a budget, pause or enable) are `external_effect` marked as approved by the marketing plan: the hook lets them run while a plan is active, and Farik's own server refuses every write the active plan does not cover (`not_in_marketing_plan`), never asks, so money moves only through a plan the owner approved. Only Farik's own servers may carry that mark, since only they can be trusted to check the plan. Nothing in the connection deletes, changes billing, account access or conversion tracking.

**The budget is a hard stop.** Each campaign is created paused, with the plan's end date, and with a budget Google itself keeps within the plan: a total budget where Google offers one for the campaign, else a daily budget no larger than what is left divided by the days left, which bounds Google's own charging when Farik is not running. While Farik runs, a tick with no model reads the spend every 15 minutes; when a campaign or the plan reaches its budget, Farik pauses the plan's campaigns itself, records `marketing_budget.reached`, and Today asks the owner to raise the budget or end the plan. No mode, no allowance and no agent goes past it.

**Paid Instagram ads wait for Meta**, a candidate once Meta lists Farik's client; until then Instagram is organic posts through Buffer.

## Consequences

Easier: a business gets a marketing function that researches, plans, posts and advertises on its own, inside a plan and a budget the owner read and approved, with each post visible before it goes out.

Harder: Farik makes calls to a service itself, not only through an agent (Buffer's post and delete, Google's spend reads), with the user's grant; that is new plumbing, which the DevOps Engineer's watch tick (step 11) also needs. Google's verification of the `adwords` scope (a sensitive scope) has a lead time of days to weeks and needs step 03e's homepage and privacy policy; until verified, only named test users sign in, so it is a launch dependency for Google Ads, not for posting. Google may spend up to one tick's worth past a cap while Farik runs, and Google's own budgets bound it while Farik does not; the spec says both.

Phase 7 gains steps 08c to 08f after 08b (Buffer and Kit): 08c the brand and the marketing plan; 08d posting through the plan; 08e Google's sign-in; 08f Google Ads with the budget's hard stop. The kit check's marketing task (step 13) becomes a plan with a small Google Ads budget, approved, with a post sent by the plan and its ads paused at the cap. `docs/design/marketing-specialist.md` holds the design.
