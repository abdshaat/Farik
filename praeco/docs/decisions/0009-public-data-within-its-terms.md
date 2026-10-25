# 0009. Public data, within its terms

Date: 2026-10-24
Status: proposed

## Context

The founder wants the agent "scraping the web about the product or the problem it's trying to solve". Catervas's marketing design (`docs/design/marketing-agent.md`, "Sources, within their terms", 2026-10-18) found: Claude's web fetch refuses reddit.com, Reddit's API needs approval and, for business use, a contract, and Exa's search returns threads with their comments; LinkedIn forbids scraping; X charges per read; Meta's terms forbid automated access to its ad library. A product that collects from places that forbid it is one complaint from being blocked, and a public repository that does it teaches everyone who forks it to.

## Decision

Praeco reads only what anyone can read without signing in, as each source's terms and `robots.txt` allow (SPEC 5.8):

- Pages through `read_page`: blocked list checked, `robots.txt` for the user agent `PraecoBot/<version> (+https://<site>/bot)` checked and cached for 24 hours, one request per host per second, 2 MB at most, text extracted before the agent sees it.
- Built-in blocked list, which no one can shorten: `linkedin.com`, `facebook.com`, `instagram.com`, `x.com`, `twitter.com`, `tiktok.com`, and each one's subdomains. Their pages may appear as search results; Praeco records the result's URL and snippet and never fetches the page.
- Reddit through Exa's search with contents (`exa_search`), Hacker News through its public Algolia API (`hn_search`), Stack Exchange through its public API (`stackexchange_search`). Each is a connector with its own tool and its own rate limit.
- No logins, no CAPTCHA solving, no rotating addresses, no headless browser.

## Consequences

Every source Praeco uses can be named in its documentation without embarrassment, and an owner can add to the blocked list for their own reasons.

What cuts against it: the agent sees less of LinkedIn and X than a person would, and those are where some audiences are. The workaround is the one Catervas chose: the owner pastes what they want read, and Praeco reads what was pasted as untrusted text.
