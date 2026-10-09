# The Marketing Specialist: brand, plan, social presence and budget

Status: approved by the founder on 2026-10-05, in conversation (ADR 0042). It is the design input to phase 7 steps 08b to 08g.

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
  starts_on, ends_on,        // ISO dates, at most 92 days, starts_on no earlier than yesterday's UTC date (so an owner west of UTC is not refused their own today)
  currency,                  // ISO 4217, the ad account's
  budget: { total, google_ads },
  campaigns: [ { key, channel: "google_ads", name, goal, advertises, budget, starts_on, ends_on } ],   // 0 to 10; advertises: what it sells, 3 to 200 characters (step 08g)
  posts: [ { key, channel, on, topic } ],     // 0 to 200; channel: instagram, x, facebook, linkedin, threads, bluesky, tiktok, pinterest, youtube, google_business, mastodon
  measures: [ "..." ] }      // 1 to 10
```

Farik checks it (each campaign within the plan's dates, campaign budgets adding up to no more than the channel's, unique keys, the channels among Buffer's) and records `marketing_plan.proposed` with a plan id, `MP-<n>`. Today shows "Marketing plan to approve": the summary, the budget by channel and campaign, the post calendar by week, and the full text, with "Approve" and "Send back" (a reason). `marketing_plan.approved` or `marketing_plan.returned` follow; a returned plan's reason reaches the agent's next session as the owner's own words, unwrapped and not as untrusted text, since they are the human's (ADR 0011). Approving is the owner's alone, under `ask` and `auto` alike.

An approved plan is active from `starts_on` to `ends_on`. One plan is active at a time; approving a new one ends the active one at the new one's start (`marketing_plan.ended`, `replaced`). The owner may end a plan on its page or with `farik marketing plan end MP-<n>` (`ended`, `by_owner`): Farik pauses its campaigns and stops its posts not yet sent. A plan also ends at its `ends_on` (`expired`). The plan's page shows its budget, what is spent, its posts (sent, waiting, stopped) and its campaigns.

## Posts

The agent writes a post with `farik_schedule_post { channel, buffer_channel, text, media, at, details?, slot? }`: `buffer_channel` is Buffer's id for the channel, which the agent reads with Buffer's `list_channels` and Farik checks with its own `get_channel`; `text` is within the network's length; `media` is up to four `{ url, kind }`, each a public `https` address the owner gave the agent or the agent made with its creative services, which Farik checks answers with a picture or a clip, at scheduling and again at hand-over (the founder's decision of 2026-10-06, "Generated, and mine"); `at` is RFC 3339 with an offset; `details` is `{ title, category_id }` for YouTube and `{ board }` for Pinterest, and no other network takes it.

- **In the plan.** `slot` names an unused post slot of the active plan with the same channel, and `at` falls on the slot's day in `at`'s own offset (`post_off_its_day`; Farik keeps no time-zone table, so the offset is the agent's choice, and the skill tells it to use the business's own) and at least three hours from now (`post_too_soon`). Farik records `social_post.scheduled` (`approved_by: plan`). Today lists it under "Going out", with its text, pictures, time and channel, and a Stop button. One hour before `at`, Farik calls Buffer's `create_post` itself, with the user's Buffer connection, scheduled for `at`, and records `social_post.sent` with Buffer's post id. Stop before then records `social_post.stopped`; Stop after it, while the post has not gone out, also calls Buffer's `delete_post`. A post whose hand-over time passed while Farik was not running is sent if `at` is still ahead, else `social_post.missed`, and the agent is told in its next session.
- **Outside the plan.** No `slot`: `social_post.requested`, shown on Today with "Post it" and "Don't post" under `ask`; under `auto` it is sent as ADR 0041 lets any outward act.
- Buffer's `create_post` and `edit_post` are `denied` to the agent from step 08d, so a post never bypasses the plan; Buffer's reads stay `network`.
- A failure at Buffer is `social_post.failed` with Buffer's message, shown on Today.

## Google Ads

A Farik connector of its own (ADR 0038), `google-ads`, run as `farik connector google-ads`, signed in with Google through Farik's own Google app, which Farik Cloud holds from phase 8, before the web launch (ADR 0044, 2026-10-06, and ADR 0048, 2026-10-08; it replaced ADR 0043's user's own app, step 03f, which is dropped), with ADR 0035's Google design, pulled forward to step 08e, scope `https://www.googleapis.com/auth/adwords` alone.

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

**The hard stop.** A campaign's budget is a total budget where Google offers one for it; otherwise a daily budget no larger than what the plan campaign has left divided by its days left, so Google's own charging limit bounds the spend while Farik is not running. While Farik runs, a watch of its own beside the ticks, with no model, reads each active plan's cost every 15 minutes (a `Search` for each ad account, through the daemon's own connection, not a session's call). When a campaign's cost reaches its budget, or the plan's reaches its Google Ads budget, Farik pauses the campaigns concerned itself, records `marketing_budget.reached`, and Today asks the owner to "Raise the budget" (a new version of the plan to approve, whose request skips the sprint queue) or "End the plan". A plan that ends has the campaigns the active plan does not carry paused within a minute, and removing Google Ads pauses them first. Google reports cost up to about an hour late, so a campaign at a daily budget may spend about an hour past its cap while Farik runs; a total budget is never passed, which is why each campaign says what it advertises and shows whether its price is fixed before the owner approves (step 08g; SPEC 6.5 has it as built).

## Skills

The kit's skills after step 08 and 08b, gaining in 08c, 08f and 08g:

| Skill | Step | What it teaches |
|---|---|---|
| `posting-and-email` | 08b | what Buffer and Kit are for, posts waiting for the human until 08d, email drafts sent from Kit, never a subscriber's data |
| `keeping-the-brand-kit` | 08c | what the kit holds, how to build it from the business's existing material, naming and describing the user's own logo and pictures, and asking for any that is missing |
| `writing-the-brand-persona` | 08c | the persona's parts, sample replies, per-network differences, what it never says |
| `researching-the-market` | 08c | audience, competitors, search words, channel costs, each fact sourced and dated |
| `writing-the-marketing-plan` | 08c | from research to goals, channels, budget split, calendar, campaigns and measures; proposing it; what the owner sees. 08g: what each campaign advertises, and a preference for a fixed price |
| `running-social-channels` | 08d | cadence per network, formats and lengths, scheduling inside the plan's slots, reading results, never a customer's data |
| `running-search-ads` | 08f | keywords and match types, negatives, ads, budgets and bids, reading search terms and cost per result, pausing what does not work. 08g: campaigns at a fixed price (3 to 90 days, made two days ahead), and raising a paused campaign's budget once the owner approves the raised version |
| 08g | The budget's hard stop: the watch and its 15-minute spend read across ad accounts, the pause at a cap and on a plan's end, removing Google Ads pausing first, `marketing_budget.reached` and `marketing_campaign.paused`, what each campaign advertises and whether its price is fixed, Today's raise or end and the raise's request that skips the sprint queue (mocked up first; SPEC 6.5) |

## Events

`marketing_plan.proposed`, `marketing_plan.approved`, `marketing_plan.returned`, `marketing_plan.ended`; `social_post.scheduled`, `social_post.requested`, `social_post.sent`, `social_post.stopped`, `social_post.missed`, `social_post.failed`; `marketing_campaign.created` (08f); `marketing_budget.reached` and `marketing_campaign.paused` (08g).

## Steps

| Step | What |
|---|---|
| 08b | Buffer and Kit in the kit (posts asking per call until 08d; Kit's broadcasts drafted inside an allowance, sent by the human from Kit); the role's "never publishes" lines reworded |
| 08c | The brand and the marketing plan: the role's mandate, four skills, `farik_propose_marketing_plan`, the plan's events, page and Today gate (mocked up first), `farik marketing plan`, `marketing_paths_owned` |
| 08d | Posting through the plan: Farik calling a service itself, `farik_schedule_post`, the hold and Stop, `social_post.*`, Today's "Going out" (mocked up first), Buffer's writes `denied` to the agent, the skill `running-social-channels` |
| 08e | Google's sign-in, pulled forward: route 2 for Google, the founder's Google Cloud project, the `adwords` scope, verification as a launch dependency (amended 2026-10-06 by ADR 0043: the user's own Google Cloud project and Desktop client, step 03f; verifying an app of Farik's is phase 17's; and by ADR 0044: Farik's own app again, held by Farik Cloud from phase 8 (ADR 0048), where its verification and the live sign-in are; step 03f dropped; Task 8 removes the build-time secret) |
| 08f | Google Ads: Farik's own `google-ads` connector, the plan mark, the skill `running-search-ads` (amended 2026-10-06 by ADR 0044: built and tested against a fake Google Ads server; no user can sign in to Google until Farik Cloud runs, so the founder's live check is phase 8's, on Farik Cloud's shared quota) |

## Not now

- Paid Instagram and Facebook ads: Meta's official Ads server admits only listed clients; a candidate once Meta lists Farik, which is phase 17's (ADR 0043), or once the user's own Meta app can be used (amended 2026-10-06 by ADR 0044: once Meta lists Farik's client, which Farik Cloud would hold, from phase 8; the user's own app is no longer a route).
- Replying to comments and messages: Buffer has no tool for it, and replying as the brand to customers is a decision the founder has not made.
- X's own server: it cannot post, and X charges per post.
