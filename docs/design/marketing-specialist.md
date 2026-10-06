# The Marketing Specialist: brand, plan, social presence and budget

Status: approved by the founder on 2026-10-05, in conversation (ADR 0042). It is the design input to phase 7 steps 08b to 08f.

## Why

The founder wants the Marketing Specialist to own the business's marketing, not only to write about it: the brand kit, the brand persona, the marketing plan built from research, and the social media presence, with write access to Instagram, X and Google Ads, spending a marketing budget by a plan the owner approved. Farik's rule that an outward act asks (spec 5.6) stays; what changes is that the owner's approval of a plan is the approval of the acts in it, as a sprint's start approves its deploys (spec 6.9).

## What the role owns

| Thing | Where | What it holds |
|---|---|---|
| Brand kit | `docs/marketing/brand/brand-kit.md`, `docs/marketing/brand/assets/` | the name and promise; the audience; colours as hex values with where each is used; type; the logo, its files and how it may be used; the picture style; the voice in three words with a do and a don't each |
| Brand persona | `docs/marketing/brand/persona.md` | who the brand is when it speaks: its character, how it talks to customers, what it never says, sample replies, and how it differs per network |
| Marketing plan | `docs/marketing/plans/MP-<n>.md` | the plan's text as proposed; the research it rests on under `docs/marketing/research/` |
| Social presence | the business's channels, through Buffer | the posts Farik sent for the plan, each recorded as an event |

These are ordinary documents under the team's `document_paths`, committed by the role's tasks and reviewed by the Product Manager. While the team has an active Marketing Specialist, the Definition of Ready refuses a task of another role that names `docs/marketing/**` in its `allowed_paths` (`marketing_paths_owned`); everyone may read them. The UI/UX Designer's `brand-and-design-tokens` takes the colours and voice from the brand kit when it exists.

The brand's logo and pictures are the business's own (the founder, 2026-10-06: "The logos are user independent not Farik's. The user will provide his own logos and images."): the user puts them in `docs/marketing/brand/assets/`, and the role keeps the brand kit's text, which names and describes them, and asks the user for any that are missing. Farik does not save generated pictures into the brand kit.

## Research

The role researches with the web (its `network` tier), its connected services' reads (Google Ads' keyword ideas and past results, Buffer's post results, Kit's email results) and the business's own documents. The skill `researching-the-market` asks for the audience and where it spends time, three to five competitors and their channels, prices and ads, the words customers search for, what each channel costs to reach someone, and each fact with its source and day.

## The marketing plan

The agent proposes a plan in the `implement` session of a marketing task with `farik_propose_marketing_plan`:

```
{ title, summary,            // summary: 20 to 600 characters, for the owner
  text,                      // the plan, 200 to 16,000 characters, also written to docs/marketing/plans/
  starts_on, ends_on,        // ISO dates, at most 92 days, starts_on not in the past
  currency,                  // ISO 4217, the ad account's
  budget: { total, google_ads },
  campaigns: [ { key, channel: "google_ads", name, goal, budget, starts_on, ends_on } ],   // 0 to 10
  posts: [ { key, channel, on, topic } ],     // 0 to 200; channel: instagram, x, facebook, linkedin, threads, bluesky, tiktok, pinterest, youtube, google_business, mastodon
  measures: [ "..." ] }      // 1 to 10
```

Farik checks it (each campaign within the plan's dates, campaign budgets adding up to no more than the channel's, unique keys, the channels among Buffer's) and records `marketing_plan.proposed` with a plan id, `MP-<n>`. Today shows "Marketing plan to approve": the summary, the budget by channel and campaign, the post calendar by week, and the full text, with "Approve" and "Send back" (a reason). `marketing_plan.approved` or `marketing_plan.returned` follow; a returned plan's reason reaches the agent's next session as untrusted text. Approving is the owner's alone, under `ask` and `auto` alike.

An approved plan is active from `starts_on` to `ends_on`. One plan is active at a time; approving a new one ends the active one at the new one's start (`marketing_plan.ended`, `replaced`). The owner may end a plan on its page or with `farik marketing plan end MP-<n>` (`ended`, `by_owner`): Farik pauses its campaigns and stops its posts not yet sent. A plan also ends at its `ends_on` (`expired`). The plan's page shows its budget, what is spent, its posts (sent, waiting, stopped) and its campaigns.

## Posts

The agent writes a post with `farik_schedule_post { channel, text, media, at, slot? }`: `text` within the network's length, `media` up to four addresses from its kit's services, `at` an ISO time.

- **In the plan.** `slot` names an unused post slot of the active plan with the same channel, and `at` falls on the slot's day (the user's time zone) and at least three hours from now (`post_too_soon`). Farik records `social_post.scheduled` (`approved_by: plan`). Today lists it under "Going out", with its text, pictures, time and channel, and a Stop button. One hour before `at`, Farik calls Buffer's `create_post` itself, with the user's Buffer connection, scheduled for `at`, and records `social_post.sent` with Buffer's post id. Stop before then records `social_post.stopped`; Stop after it, while the post has not gone out, also calls Buffer's `delete_post`. A post whose hand-over time passed while Farik was not running is sent if `at` is still ahead, else `social_post.missed`, and the agent is told in its next session.
- **Outside the plan.** No `slot`: `social_post.requested`, shown on Today with "Post it" and "Don't post" under `ask`; under `auto` it is sent as ADR 0041 lets any outward act.
- Buffer's `create_post` and `edit_post` are `denied` to the agent from step 08d, so a post never bypasses the plan; Buffer's reads stay `network`.
- A failure at Buffer is `social_post.failed` with Buffer's message, shown on Today.

## Google Ads

A Farik connector of its own (ADR 0038), `google-ads`, run as `farik connector google-ads`, signed in with Google through Farik's Google app (ADR 0035's Google design, pulled forward to step 08e), scope `https://www.googleapis.com/auth/adwords` alone.

| Tool | Tag | What it does |
|---|---|---|
| `list_accounts` | `network` | the ad accounts the sign-in reaches, with their currency and time zone |
| `report` | `network` | one of a fixed set of reports (campaigns, ad groups, keywords, search terms, ads) with clicks, impressions, cost and conversions over a date range; no free query |
| `keyword_ideas` | `network` | keyword ideas with their search volume and bid range, for research |
| `create_search_campaign` | `external_effect`, plan | a Search campaign for one plan campaign: created paused, its end date the plan campaign's, its budget within the plan campaign's left |
| `add_ad_group`, `add_keywords`, `add_negative_keywords`, `add_responsive_search_ad` | `external_effect`, plan | what a Search campaign needs, only under a campaign Farik created for the active plan |
| `set_campaign_budget` | `external_effect`, plan | a new budget within what the plan campaign has left |
| `set_campaign_status` | `external_effect`, plan | pause, or enable within the plan campaign's dates and with budget left |

"Plan" means the tool is marked approved by the marketing plan: the hook runs it while a plan is active, and Farik's server refuses (`not_in_marketing_plan`) every call the active plan does not cover. Nothing deletes, and nothing touches billing, account access, conversion tracking, other campaign types or other accounts' campaigns. Other campaign types (Performance Max, Demand Gen, video) are candidates for a later plan.

**The hard stop.** A campaign's budget is a total budget where Google offers one for it; otherwise a daily budget no larger than what the plan campaign has left divided by its days left, so Google's own charging limit bounds the spend while Farik is not running. While Farik runs, a tick with no model reads each active plan campaign's cost every 15 minutes (`report`, through Farik's own call). When a campaign's cost reaches its budget, or the plan's total reaches the plan's, Farik pauses the campaigns concerned itself, records `marketing_budget.reached`, and Today asks the owner to "Raise the budget" (a new version of the plan to approve) or "End the plan". The spec says Google may spend up to one tick's worth past a cap while Farik runs.

## Skills

The kit's skills after step 08 and 08b, gaining in 08c and 08f:

| Skill | Step | What it teaches |
|---|---|---|
| `posting-and-email` | 08b | what Buffer and Kit are for, posts waiting for the human until 08d, email drafts sent from Kit, never a subscriber's data |
| `keeping-the-brand-kit` | 08c | what the kit holds, how to build it from the business's existing material, keeping every asset with its source |
| `writing-the-brand-persona` | 08c | the persona's parts, sample replies, per-network differences, what it never says |
| `researching-the-market` | 08c | audience, competitors, search words, channel costs, each fact sourced and dated |
| `writing-the-marketing-plan` | 08c | from research to goals, channels, budget split, calendar, campaigns and measures; proposing it; what the owner sees |
| `running-social-channels` | 08d | cadence per network, formats and lengths, scheduling inside the plan's slots, reading results, never a customer's data |
| `running-search-ads` | 08f | keywords and match types, negatives, ads, budgets and bids, reading search terms and cost per result, pausing what does not work |

## Events

`marketing_plan.proposed`, `marketing_plan.approved`, `marketing_plan.returned`, `marketing_plan.ended`; `social_post.scheduled`, `social_post.requested`, `social_post.sent`, `social_post.stopped`, `social_post.missed`, `social_post.failed`; `marketing_budget.reached`.

## Steps

| Step | What |
|---|---|
| 08b | Buffer and Kit in the kit (posts asking per call until 08d; Kit's broadcasts drafted inside an allowance, sent by the human from Kit); the role's "never publishes" lines reworded |
| 08c | The brand and the marketing plan: the role's mandate, four skills, `farik_propose_marketing_plan`, the plan's events, page and Today gate (mocked up first), `farik marketing plan`, `marketing_paths_owned` |
| 08d | Posting through the plan: Farik calling a service itself, `farik_schedule_post`, the hold and Stop, `social_post.*`, Today's "Going out" (mocked up first), Buffer's writes `denied` to the agent, the skill `running-social-channels` |
| 08e | Google's sign-in, pulled forward: route 2 for Google, the founder's Google Cloud project, the `adwords` scope, verification as a launch dependency |
| 08f | Google Ads: Farik's own `google-ads` connector, the plan mark, the spend tick and the hard stop, `marketing_budget.reached`, Today's raise or end, the skill `running-search-ads` |

## Not now

- Paid Instagram and Facebook ads: Meta's official Ads server admits only listed clients; a candidate once Meta lists Farik.
- Replying to comments and messages: Buffer has no tool for it, and replying as the brand to customers is a decision the founder has not made.
- X's own server: it cannot post, and X charges per post.
