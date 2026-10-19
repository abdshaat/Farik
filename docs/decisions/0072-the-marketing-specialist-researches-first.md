# 0072. The Marketing Specialist researches the product, its people and its rivals before it markets

Date: 2026-10-18
Status: accepted (the founder, 2026-10-18 and 2026-10-19, quoted in `docs/design/marketing-agent.md`; the mechanism decided by the planner)

Amends:
- **ADR 0042** (the Marketing Specialist owns the business's marketing): the role keeps everything it owns and does, and gains a research foundation it runs before its first plan, and refreshes with every plan.
- **SPEC 6.5** (revision 0.127).

## Context

The founder asked for "a special single marketing agent" that first researches the product, then the people it is for ("must scrape public information online such as linkedin, X, and most importantly reddit forums"), then its competitors, sorted into direct ("almost identical to our product") and indirect ("a similar product"), then how those competitors reach people, "what posts are they posting, and what type of customers do they target", and that ends knowing "where to start and how to market the current product"; it must also do SEO, make posters and videos and manage social media. Asked by the live team's Product Manager, the founder answered "Replace the Marketing Specialist" and then "Keep everything, add the missing parts". Then: "Research public marketing skills that are proven to work … Then checkout the repositories to construct a marketing agent skills."

Three facts constrain it (research of 2026-10-18, `docs/design/marketing-agent.md`):
- Reddit's own API needs Reddit's approval for every new token and a contract for commercial use, and Claude's `WebSearch` and `WebFetch` refuse reddit.com. Exa's keyless search and page reader do return Reddit threads with their comments (checked 2026-10-18).
- LinkedIn forbids scraping, and won its case against it; X's API is paid per read with the user's own developer app.
- An SEO data service (DataForSEO) has no tool list Catervas can pin without an account.

## Decision

1. **The same role, rewritten in place.** The agent is the Marketing Specialist, id `marketing_specialist`: no new role, no migration, no team file changes; every team that has one, the live team's Kai included, gets the new agent. Rejected: a second marketing role beside it (the founder chose to replace it); a new id with a migration (it would change nothing the owner sees but the files).
2. **A research foundation first.** Before its first plan, and again before each plan after, the agent runs five stages, each one document under `docs/catervas/marketing/research/`: the product (`product`), the people it is for (`audience`), the competitors in three groups, direct, indirect, and substitutes and doing nothing (`competitors`), how they reach people (`outreach`), and where to start (`positioning`, with the channels to test first). The plan cites them. A stage whose document is under 92 days old is refreshed, not redone.
3. **Public sources, read within their terms.** Reddit through Exa (search, and its page reader for a whole thread), threads the owner pastes, and Reddit's own free Trends for business, which the owner copies in; X and LinkedIn through search results of their public pages and the owner's own exports. Ad libraries (Meta's, Google's, LinkedIn's, TikTok's) the owner opens and pastes or screenshots, since Meta's terms forbid automated access. The agent never signs in to a forum or a network as anyone, never collects people's names, and quotes short excerpts with their links. Rejected: Reddit's API (closed to commercial use), scraping LinkedIn (forbidden, and lost in court), X's API (the owner's developer app and pay per read; later, as a connection).
4. **Exa joins the kit, both tools `network`.** The Procurement Specialist denies Exa's page reader because it follows redirects past its approved sites; the Marketing Specialist has no approved sites and already holds `WebFetch`, so the reader adds no reach it lacks. Rejected: Exa's search alone (it shows a thread's top comments, not the thread).
5. **SEO from what Catervas can read.** Search words from Google Ads' keyword ideas, search results, the competitors' pages and the audience's own words; a check of the business's site from its pages and its code; briefs and pages written for search and for AI answers. Rejected for now: DataForSEO, whose tools Catervas cannot pin without an account.
6. **Posters are drawn, not generated.** A poster is an SVG under `docs/catervas/marketing/assets/posters/`, laid out from the brand kit's colours, type and the owner's own logo, its text exact; a generated picture may sit inside it. Video stays Higgsfield's, with a written storyboard first. Rejected: a generated poster (its text and colours drift).
7. **Skills built from proven public ones.** The new and rewritten skills draw on public marketing skill sets with a record of use, credited in each skill under their licences; nothing from them that assumes tools Catervas does not have, scraping, or posting without the owner's plan.
8. **A question in the chat.** The chat answers from the foundation's documents; a question that needs research the agent takes as its own task (`catervas_take_task`), since a chat has 12 tool calls and 3 minutes.

## Consequences

- Positive: the agent plans from evidence about the product, its people and its rivals, each fact sourced and dated; Reddit, the founder's first source, is read.
- Negative: the first plan waits for five stages of research, a few dollars of tokens; Reddit's coverage is Exa's index, not Reddit's whole.
- Neutral: everything ADR 0042 built (the plan, the posts and their Stop, Google Ads and its hard stop, the brand kit and persona, Kit's drafts) is unchanged.
