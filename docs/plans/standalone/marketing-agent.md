# Standalone: the marketing agent researches first

Status: ready
Branch: `feat/marketing-agent` (its own pull request to `main`)
Design: `docs/design/marketing-agent.md`; ADR 0072
Spec: `docs/SPEC.md` 6.5 and 4.5's Files names (revision 0.127)
Depends on: `origin/main` at `9cd0f5c0`
Readiness confirmed by: the planner (Opus 5.5), 2026-10-18, against the three readiness rules (ADR 0065)

## Goal

The founder's brief (design, "The brief"). When done: the Marketing Specialist runs a research foundation (the product, its people, its competitors in three groups, how they reach people, where to start) before it plans, each stage a document under `docs/catervas/marketing/research/` that the Files page names; it reads Reddit through Exa; it has skills for SEO and posters; its chat answers from the foundation and takes a research question as its own task. Out of scope: DataForSEO, X's API, replies posted by Catervas, any change to the plan's tools, posts, Google Ads or the budget's stop.

## Decisions

- Same role, rewritten in place: ADR 0072 decision 1.
- Exa in the kit with both tools `network`: ADR 0072 decision 4. The entry is the Procurement kit's, with marketing words in `why` and `setup`, and `web_fetch_exa: network`. Exa's live listing on 2026-10-18 is exactly `web_search_exa`, `web_fetch_exa`.
- Skills rewritten from public sets, never copied; each ends with a "Drawn from" section naming the set, its author and its licence: design, "Skills". Rejected: vendoring the sets' files with their licences (the text assumes tools Catervas lacks; rewriting is shorter than editing).
- A skill is one `SKILL.md`, a `name` and a `description` that starts "Use when", numbered sections, under 6 KB, as the kit's other skills (the kit test holds the size).
- The new skills sit before `catervafying-the-repository`, which stays last (SPEC 6.5).
- The chat's behaviour is the prompt's (`system.md`, "Questions in your chat"), not code: the chat tools exist (ADR 0063).
- Five new `DOCUMENT_STEMS`, after "Marketing kickoff": `marketing/research/product` "The product", `audience` "The people it is for", `competitors` "Competitors", `outreach` "How competitors reach people", `positioning` "Where to start".
- One test per major functionality (ADR 0060): the kit test (skills and Exa) and the names test are extended; prompts and skill prose are not asserted beyond their headers.

## File map

```
crates/roles/roles/marketing_specialist/system.md          modifies: the foundation first, the sources, the chat
crates/roles/roles/marketing_specialist/role.yaml          modifies: mandate and produces name the foundation
crates/roles/roles/marketing_specialist/kit.yaml           modifies: seven skills listed, Exa added, header comment
crates/roles/roles/marketing_specialist/skills/<7 new>/SKILL.md   creates
crates/roles/roles/marketing_specialist/skills/researching-competitors/SKILL.md     rewrites
crates/roles/roles/marketing_specialist/skills/writing-the-marketing-plan/SKILL.md  modifies: first 30 days, stages
crates/roles/roles/marketing_specialist/skills/running-social-channels/SKILL.md     modifies: drafted replies
crates/roles/src/kit.rs                                     modifies: embedded! list; tests: the kit test
crates/core/src/folders.rs                                  modifies: DOCUMENT_STEMS; tests: the names test
docs/SPEC.md                                                modifies: 6.5, 4.5 names, revision 0.127
docs/design/marketing-specialist.md                         modifies: a line that 0072 amends it
```

## Interfaces

Consumes: `load_kit`, `Role::MarketingSpecialist`, `embedded!` (`catervas-roles`, on main); `DOCUMENT_STEMS`, `document_name`, `document_order` (`catervas-core`, on main).

Produces: `DOCUMENT_STEMS: [(&str, &str); 20]`. No new function.

## Tasks

### Task 1: the kit carries the seven skills and Exa

Files: created the seven `SKILL.md`; modified `kit.yaml`, `kit.rs`, the three rewritten skills.

Tests, watched to fail before the skills exist:

- `marketing_kit_carries_running_search_ads` (extended) — asserts the kit's skill names in order are the sixteen of today with `running-the-research-foundation`, `understanding-the-product`, `researching-the-audience`, `studying-competitor-outreach`, `finding-where-to-start`, `optimising-for-search`, `making-posters` inserted after `running-search-ads` and before `catervafying-the-repository`; each of the seven's description starts with its "Use when" line from the design and names the skill or path beside it (`running-the-research-foundation` names `docs/catervas/marketing/research/`; `researching-the-audience` names `web_search_exa`; `optimising-for-search` names `writing-for-search`; `making-posters` names `docs/catervas/marketing/assets/posters/`; `studying-competitor-outreach` names `research/competitors`; `finding-where-to-start` names `research/positioning`; `understanding-the-product` names `research/product`); each is under 6 KB and has a "Drawn from" section; and the kit's connectors include `exa` with `web_search_exa` and `web_fetch_exa` both `network`.

- [x] `feat(roles): the Marketing Specialist researches its product, people and rivals first`

### Task 2: the prompt and the role

Files: modified `system.md`, `role.yaml`. No new test (prose; Task 1's kit test loads the role).

- [x] `feat(roles): the Marketing Specialist's prompt runs the foundation and answers its chat from it`

### Task 3: the Files page names the five documents

Files: modified `folders.rs`.

- `names_the_known_documents_and_the_rest_plainly` (extended) — asserts `DOCUMENT_STEMS.len()` is 20, `marketing/research/audience.html` is named "The people it is for", and `marketing/research/product.md` sits right after `marketing/kickoff.md` with the other four after it in the design's order.

- [ ] `feat(core): name the marketing research documents on the Files page`

### Task 4: spec and design

- [ ] `docs(spec): record the marketing agent's research foundation (0.127)`

## Verification

```
cargo xtask check
# expected: xtask check: ok
```

The founder's own test, in the web app: ask the Marketing Specialist in its chat, among others, "Where should I start marketing this?" and "What do people on Reddit say about tools like ours?"; it takes them as tasks and the documents appear under Files, Marketing.
