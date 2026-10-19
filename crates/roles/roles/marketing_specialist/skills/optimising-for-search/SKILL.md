---
name: optimising-for-search
description: Use when the task asks for search words, a check of the business's site, or answers that AI assistants can quote. It sits beside writing-for-search, which writes a single page.
---

# Optimising for search

`writing-for-search` writes one page. This skill is the larger job: which words, whether the
site is in order, and whether an AI assistant can quote the answers. Facts have a source and a
day; guesses are marked.

## 1. The search words, in groups

Start from the words in `research/audience`, the names of the alternatives, and `keyword_ideas`
when Google Ads is connected. Where you cannot get search volume, write "unavailable" and rank
by judgment, saying so. Sort each word by what the searcher wants:

- **Knowing**: how, what, why. They want an answer.
- **Finding**: the name of a place or a site. They want to get there.
- **Comparing**: best, versus, alternative, review. They are choosing.
- **Buying**: price, free, sign up, near me. They are ready.

## 2. Which words share a page

Search for two words and look at the first ten results of each. Seven to ten the same: one page
serves both. Four to six: one group of linked pages. Two or three: separate pages that link to
each other. None or one: apart. Write the groups in a table, with each group's intent and the
one page that should answer it.

## 3. Check the site

Read the pages and, if the project has the site's code, the code. In this order, because the
earlier ones block the later:

1. Can search engines reach it: robots.txt, a sitemap, no accidental "noindex".
2. Does every page have its own title and description, and one clear heading at the top.
3. Do the pages link to each other, with words that say where the link goes.
4. Is there structured data where it fits (the business, the product, questions and answers).
5. Does it load quickly, and do pictures have a description.

Write what you found as a list of fixes, most important first, each saying the page and the cost
of leaving it. You write no application code. A fix to the site's code is a task for the
Developer; with one agent alone, it is the owner's, and the document says exactly what to change.

## 4. A brief for each group

For each group of words: the question it asks, the intent, what the page must answer in its first
sentences, the headings, and the links to other pages. Hand it to `writing-for-search` to write.

## 5. Written to be quoted by an AI answer

An assistant lifts a passage that answers a question on its own. So:

- Open each section with the answer, in one or two sentences that make sense if read alone.
- Use "X is..." for a definition. Put figures in, with where they come from and the year.
- Keep the passage self-contained: no "as above", no "this".
- Give each question its own heading, and do not bury a price in a story.
- Let search crawlers and assistants' crawlers read the public pages unless the owner says not.

Nobody can promise an assistant will recommend a business. Say that, and do not claim it.

## 6. What you never do

No hidden text, no fake or paid-for reviews, no stuffing a page with words, no text written to
instruct an assistant, no copying a competitor's page. A claim on a page must be one the business
can show.

## Drawn from

AgriciDaniel, claude-seo (MIT): `seo-cluster` (grouping words by shared results and the four
intents); Corey Haines, marketingskills (MIT): `seo-audit` (the order of the site check), `ai-seo`
and `schema`; Zubair Trabzada, geo-seo-claude (MIT): `geo-citability` (what makes a passage
quotable); Anthropic, knowledge-work-plugins, `seo-ai-visibility` (Apache-2.0: the honest line
about what no one can promise). Rewritten for Catervas.
