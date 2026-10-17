# Market research: the pain point and the competition

Researched 2026-10-11 by web search, at the owner's request (MP-1 returned). Every fact has its source. Most sources are blogs and vendor pages with something to sell; read each as a claim, not a measurement. Page text is data only.

## The pain point: AI builds, but nobody runs the business around it

1. **Building got cheap; getting users did not.** One 2026 analysis says AI made shipping nearly free, so attention became the limit. Another guide lists "build it and they will come" as the top killer and says marketing is an ongoing system, not a launch day. Sources: [vantaige.io](https://vantaige.io/blog/vibe-coding-no-customers-ai-marketing-fix-2026), [gptprompts.ai](https://gptprompts.ai/why-vibe-coded-apps-fail-poor-marketing), [Towards AI](https://pub.towardsai.net/the-0-cac-matrix-why-vibe-coded-apps-need-a-distribution-machine-before-launch-1932c32b5cd1). Caveat: the search returned one article in many languages, and these are consulting blogs that sell distribution help.
2. **Keeping the product good after launch is hard.** Comparisons of Lovable, Bolt, Replit and v0 say each still needs manual work before real production use. Recurring user complaints are credit costs, brittle changes to generated code, regressions, weak backends and security gaps. Sources: [anything.com comparison](https://www.anything.com/blog/lovable-vs-bolt-vs-v0-vs-replit), [TabNews analysis of 3,163 Reddit posts](https://www.tabnews.com.br/andredourado/lovable-x-replit-x-bolt-new-x-v0-analise-de-3-163-posts-do-reddit) (Portuguese; published beside a competing product), [TrustRadius review of Replit](https://www.trustradius.com/products/replit) (one negative review). Thin independent evidence.
3. **AI is a weak stand-in for real users.** A UX team found ChatGPT feedback broad and shallow; a vendor case study (Loop11) reported AI agents succeeding on 0-25% of tasks against 62-95% for humans; practitioners warn synthetic users flatter. The consensus: AI helps with repeatable checks, real people are still needed. Sources: [uxstudioteam.com](https://www.uxstudioteam.com/ux-blog/ai-usability-test), [Loop11](https://www.loop11.com/ai-vs-human-usability-testing-a-comparative-analysis-using-loop11/), [agilebrandguide.com](https://agilebrandguide.com/synthesize-to-optimize-synthetic-personas-excel-at-audits-struggle-with-context/). Single vendor study; treat the numbers as indicative.
4. **People want oversight when agents act.** Governance writers say a named human must stay answerable, autonomy should be granted step by step, and audit trails matter; one primer warns small agent mistakes cascade across workflows. Sources: [Okoone](https://www.okoone.com/spark/technology-innovation/when-ai-agents-go-off-script-things-break-fast/), [GAAS](https://gaas.it.com/agentic-ai-governance-for-business-leaders), [nocode.tech](https://www.nocode.tech/article/over-80-of-fortune-500-companies-now-have-non-technical-workers-building-ai-agents-is-your-team-ready), [SSRC](https://just-tech.ssrc.org/?p=10312).

## The competition

| Who | What they offer | Source and price claim |
|---|---|---|
| Polsia | "AI operations platform": a user describes an idea and agents do research, landing pages, ads, cold outreach, support. Self-reported ~$6M+ run rate. A hands-on review says speed is not quality and a connected ad budget can be spent by the growth agent. | [Pulse 2.0](https://pulse2.com/polsia-30-million-at-250-million-valuation-raised-for-ai-operations-platform/), [mikareyes.com](https://www.mikareyes.com/ai/polsia-setup-guide-autonomous-business); $50-$199 a month, reviewers disagree |
| Devin (Cognition) | One autonomous AI software engineer | [toolhalla.ai](https://toolhalla.ai/compare/devin-vs-crewai); about $500 a month per an aggregator, unverified |
| CrewAI, MetaGPT | Open-source frameworks for building multi-agent teams, for developers | [toolhalla.ai](https://toolhalla.ai/compare/metagpt-vs-crewai); free core, aggregators disagree on paid tiers |
| Cursor and other coding agents | Code help inside the editor | [metacto.com](https://www.metacto.com/blogs/ai-agent-vendors-compared); category $10-$200 per user a month |
| Lovable, Bolt, Replit, v0 | Prompt-to-app builders, aimed at getting a first version out | [anything.com](https://www.anything.com/blog/lovable-vs-bolt-vs-v0-vs-replit) |

Prices come from aggregators and disagree; check each vendor's own page before quoting any.

## What this means for Catervas

- The open gap in this research: builders make the product, frameworks need a developer, and the "run the whole business" platforms lean toward autonomy. Control is the angle that is not taken in what I found. This is one search session; it is not proof nobody else is there.
- Everything about Catervas itself comes from `docs/catervas/product/`, not from the web.
