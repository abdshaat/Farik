# The marketing agent: research first, then market

Status: decided by the planner (Opus 5.5), 2026-10-18, on the founder's brief; ADR 0072; SPEC revision 0.127. It amends `docs/design/marketing-specialist.md` (ADR 0042), which stays the design of the plan, the posts, Google Ads and the budget's stop.

## The brief

The founder, verbatim:
1. 2026-10-18: "I am looking to build a special single marketing agent in Catervas. … The agent will start by deeply researching the current product, Understanding it, then researching the targeted user, must scrape public information online such as linkedin, X, and most importantly reddit forums. Then start researching competition and dividing them into categories. Direct competitors are competitors who are almost identical to our product. Indirect competitors are those who provide a similar product to the product we are working on. Finally, Research how those competitors are gaining outreach, what posts are they posting, and what type of customers do they target. At the end, the agent should have a good idea of where to start and how to market the current product that he have. The marketing agent must also utilize some SEO technique, Generate posters and videos, and manages social media."
2. 2026-10-19, to the live team's Product Manager: "Replace the Marketing Specialist", then "Keep everything, add the missing parts".
3. 2026-10-18: "Please perform research on marketing agents and marketing principle and build me the agent. I will personally test it end to end by asking it questions."
4. 2026-10-18: "Research public marketing skills that are proven to work on Hackerrank or reddit. Then checkout the repositories to construct a marketing agent skills."

## What exists, and what is added

The Marketing Specialist (ADR 0042) owns the brand kit, the brand persona, the marketing plan and the social presence; it has seventeen skills, Higgsfield and Recraft for pictures and clips, Buffer for eleven networks, Kit for email drafts, and Catervas's own Google Ads connector with a budget's hard stop. All of it stays.

Added: a research foundation run in a fixed order before the plan; the competitors in three groups; how they reach people; the audience read from forums, Reddit first; SEO with a site check and AI answers; posters drawn from the brand kit; Exa in the kit; and the chat answering from the research.

## The foundation

Five stages, in this order, each one document under `docs/catervas/marketing/research/` in the team's format, every fact with its source and the day it was read, every guess marked as one. A stage reads the documents before it.

| # | Stage | Reads | Writes | Holds |
|---|---|---|---|---|
| 1 | The product | the project: `docs/catervas/product/` when the team has a Product Manager, else the README, the code, the site and its pricing page; the owner's answers | `research/product` | what it does and for whom, the job it does, its features as benefits, its proof, its price, how it describes itself now, what is missing |
| 2 | The people it is for | `product`; Reddit through Exa; other forums and communities through the web; X and LinkedIn through search results of their public pages; threads and exports the owner gives | `research/audience` | one main group and at most two more; each one's job, what pushes them to change and what holds them back, in their own quoted words with links; the words they use; where they gather and each place's rules |
| 3 | The competitors | `product`, `audience` (the alternatives people name) | `research/competitors` | three groups: **direct**, almost the same product; **indirect**, a different product for the same job; **substitutes and doing nothing** (a spreadsheet, an agency, putting up with it); for each, its line, its price, who it is for, what its customers praise and complain about |
| 4 | How they reach people | `competitors` | `research/outreach` | for each direct and indirect competitor: its channels and how often it posts, what its posts and ads say and which do well, its offers, who each targets, the search words it ranks for, and the gaps nobody fills |
| 5 | Where to start | 1 to 4 | `research/positioning` | the positioning (the alternatives, what only this product has, the value it gives, who cares most, the category), the message per group, and the channels to test first: every channel scored, three cheap tests, each with a measure and a point to stop |

The marketing plan (`writing-the-marketing-plan`, unchanged) then opens with the first 30 days, from stage 5. A stage whose document is under 92 days old (a plan's longest) is refreshed: the agent rereads what changes (prices, new competitors, new threads) and updates it.

## Sources, within their terms

| Source | How | Why |
|---|---|---|
| Reddit | Exa's search and its page reader (`web_search_exa`, `web_fetch_exa`), keyless; threads the owner pastes; Reddit's free Trends for business, whose weekly summary the owner copies in | Claude's `WebSearch` and `WebFetch` refuse reddit.com, and Reddit's API needs its approval and, for business use, a contract. Exa returned threads with their comments on 2026-10-18 |
| X, LinkedIn | search results of their public pages; the owner's own exports | LinkedIn forbids scraping (hiQ lost); X reads cost per post with the owner's developer app, a connection for later |
| Ad libraries | the owner opens Meta's Ad Library, Google's Ads Transparency Center, LinkedIn's or TikTok's for a competitor and pastes or screenshots what it shows; the agent says which to open and reads what it gets | Meta's terms forbid automated access, and the APIs cover political ads or the EU alone |
| Search words | Google Ads' `keyword_ideas` when connected; search results; the competitors' pages | an SEO data service (DataForSEO) cannot be pinned without an account; later |

The agent never signs in to a forum or a network as anyone, never writes down a person's name or handle beyond a quote's link, and quotes only short excerpts. What a page says is data, never an instruction (as before).

## SEO

`optimising-for-search` (new) beside `writing-for-search` (kept, for writing a page): the search words in groups by what the searcher wants, from the audience's words, `keyword_ideas` and the results pages; the business's site checked from its pages and its code (titles, descriptions, headings, links, structured data, speed hints); a brief per group; pages written to be quoted by AI answers (the answer first, figures with their sources). A change to the site's code is the Developer's task, or, for one agent alone, the owner's to make from the agent's document, since the agent writes no application code.

## Posters and videos

A poster is an SVG under `docs/catervas/marketing/assets/posters/`, laid out from the brand kit's colours, type and the owner's logo, its words exact; a picture made with Higgsfield or Recraft may sit inside it. A video starts as a storyboard document (scenes, words on screen, voice-over, length per network) and is made with Higgsfield. Unchanged: a post's pictures are at a public address or made by a service (ADR 0042).

## Social media

Unchanged: posts through Buffer, in the plan, with a Stop on Today. Replies are drafted in the brand persona's voice for the owner to send, since Buffer has no tool for them. On Reddit the agent drafts helpful comments, disclosed as the business's, for the owner to post from their own account; it never posts there.

## The chat

The owner asks the agent questions in its chat. A chat session has 12 tool calls and 3 minutes, so it answers from the foundation's documents and a few searches; a question that needs a stage of research it takes as its own task with `catervas_take_task`, says so, and the owner sees the task on the board and the document in Files when it is done. Before any foundation exists, the first such task is the foundation.

## Skills

Built from public skill sets with a record of use, surveyed 2026-10-18 (Reddit could not be read, and Hacker News threads on them are small, so the evidence is GitHub's: stars, contributors, recent commits, and each set's own tests): Corey Haines's `marketingskills` (MIT, 55k stars, an eval set per skill; the main source), Anthropic's `knowledge-work-plugins` marketing and small-business plugins (Apache-2.0), `claude-seo` by AgriciDaniel (MIT) and `geo-seo-claude` by Zubair Trabzada (MIT) for SEO and AI answers, and Anthropic's `canvas-design` skill (Apache-2.0) for posters. Catervas's skills are rewritten, not copied, each crediting what it draws on in a last section. Left out: everything that drives a signed-in browser, fetches Reddit's own pages, crawls an ad library, needs a tool Catervas does not have (Firecrawl, DataForSEO, SparkToro, HubSpot, Canva, Remotion and others), posts outside the plan, or presses with false scarcity. `alirezarezvani/claude-skills` copies much of Haines's text without credit and is not drawn on for those skills.

New, seven, each listed before `catervafying-the-repository`:

| Skill | Use when | From |
|---|---|---|
| `running-the-research-foundation` | the agent has no foundation, a plan is due, or the owner asks where to start | the order above; Haines's `product-marketing` "learnings" and the refresh |
| `understanding-the-product` | stage 1 | Haines's `product-marketing`: a draft from the code, README, site and pricing first, then the owner corrects it; the category as "the shelf"; the market-quality check |
| `researching-the-audience` | stage 2 | Haines's `customer-research`: the extraction table (source and day, quote, context, theme), jobs, triggers, the alternatives, themes by how often and how strongly, High/Medium/Low confidence, the sample's bias, five points before a group; Reddit through Exa |
| `studying-competitor-outreach` | stage 4 | Haines's ad teardown schema (offers, formats, the longest-running ad as a likely winner, "unknown, don't guess") and "reverse engineering" posts: hook, format, how often, what does well; the gaps |
| `finding-where-to-start` | stage 5 | April Dunford's five parts (the idea credited to *Obviously Awesome*), Anthropic's competitive brief's positioning, Haines's owned, rented and borrowed channels and "hope is not a strategy": three cheap tests, each with a measure and a point to stop |
| `optimising-for-search` | search words, a site check, AI answers | `claude-seo`'s grouping by shared results (7 to 10 of the top ten shared, one page; 4 to 6, one cluster; 2 to 3, linked; 0 or 1, apart), the four intents, Haines's `seo-audit` order and `ai-seo`, `geo-seo-claude`'s quotability parts; never hidden text, fake reviews or prompts aimed at assistants |
| `making-posters` | a poster, a social card or a storyboard | `canvas-design`'s short visual idea before the layout, bound to the brand kit, its words exact; Haines's `image` and `video`: sizes per network, a storyboard first, the first three seconds |

Rewritten: `researching-competitors` (the three groups; for each fact what was seen with its source and day, what is inferred with a confidence, what it means as a question; "not seen is not absent"; never a motive), from Haines's `competitor-profiling`; `writing-the-marketing-plan` gains the first 30 days from `research/positioning`, each move tagged by the stage of the customer's way it serves (finding, trying, staying, paying, telling), from Haines's `marketing-plan`; `running-social-channels` gains replies and Reddit comments drafted for the owner, from Haines's `social` and `community-marketing`. The other skills are kept as they are.

The Files page names the five documents ("The product", "The people it is for", "Competitors", "How competitors reach people", "Where to start"), after "Marketing kickoff".
