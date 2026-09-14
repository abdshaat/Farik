# Farik Product Analysis

Version 0.1. Companion to `SPEC.md`. Written before any code exists, so every number here is an estimate and every competitive claim is as of mid-2026 and should be re-checked before launch.

## The finding

Farik is worth building, but not for the reason that is most fun to describe. The pixel office and the named, avatar-bearing agents will get people to try it. What will make them keep it is the governance layer: contracts before work, a reviewer who is never the author, budgets that actually stop things, and a log that explains what happened. Nobody in the current market sells that layer as the product. Several sell role-based agent teams, and a few of those are good, but they treat "what stops the team from going wrong" as the user's problem.

The corresponding risk is that the governance layer is invisible in a screenshot and boring in a demo. The analysis below spends most of its length on that tension because it decides the positioning, the pricing, and the first milestone.

## The problem

People who try to run more than one coding agent at a time hit the same wall. The agents are individually capable and collectively unreliable. Cemri et al. (2025, "Why Do Multi-Agent LLM Systems Fail?", arXiv 2503.13657) studied over 150 traces across seven multi-agent frameworks and sorted the failures into three groups: specification and system design problems (the task was never well defined, roles were not enforced), inter-agent misalignment (agents ignored each other, talked past each other, or reset each other's work), and verification failures (nobody checked, or the check was wrong, or the system stopped too early). Their headline was that these are mostly not model-capability failures. They are organizational failures, of the kind human teams solved decades ago with things like definitions of done and a reviewer who is not the author.

That is the gap. The frameworks give you a way to wire agents together. They do not give you the process that stops a wired-together team from spending forty dollars re-implementing a feature that was already there, or from declaring a task done because the agent that did it said so.

A second, smaller problem is legibility. When an agent works for four hours overnight, the person who comes in the morning needs to answer "what changed, why, and what did it cost" in under five minutes. Most tools show a transcript. A transcript is the wrong artifact for that question.

## Who has this problem now

The market that exists today, not the one analysts project.

Solo builders and indie hackers running Claude Code, Cursor, Codex, or similar on side projects. They already spend on tokens and already wish the tool would keep going while they sleep. They are numerous and cheap to reach through open-source channels, and they will not pay much.

Small software companies where one person is the engineering lead and the backlog outruns the headcount. They are the ones who would pay for hosted execution and who care about the audit trail because they have to explain to a co-founder what the agents did.

Agencies and contractors who run several client projects at once and would happily keep a separate team per client. This group is speculative; it is plausible from the shape of the product but nobody has asked for it yet.

I have not sized these segments in dollars. Analyst figures for "the AI agent market" vary by an order of magnitude and mostly measure enterprise chatbots. The honest statement is that the first segment is large enough to give an open-source project a community, and the second is large enough to fund a small company if the conversion rate is ordinary.

## The landscape

Four groups of products touch what Farik does. None does the whole thing, and the differences matter for positioning.

### Role-based agent frameworks

MetaGPT (Hong et al., 2023; ICLR 2024) is the closest intellectual ancestor. It instantiates a software company with a product manager, architect, project manager, and engineers, and its key idea is that encoding standard operating procedures per role reduces the cascade of one agent's hallucination into the next agent's input. Its authors summarized it as "Code = SOP(Team)". ChatDev (Qian et al., 2023; ACL 2024) runs a similar company through a chain of pairwise conversations. CrewAI wraps the same pattern into a Python library with a hosted enterprise product. Microsoft's AutoGen (Wu et al., 2023) and its AG2 fork are the general-purpose version.

What they share with Farik: named roles with mandates. What they lack: a persistent team on a persistent project, a human in the loop by design, a governor that is code rather than prompt, and any user interface a non-programmer would open. MetaGPT and ChatDev are one-shot generators: you give a brief, they emit a repository. Farik is a team that keeps working on the same repository for months. CrewAI is closest in spirit but is a library for developers to build with, not a product to run.

### Autonomous coding agents

Devin (Cognition), OpenHands (All Hands AI, open source), Claude Code, OpenAI's Codex, Google's Jules, and Cursor's background agents. These are single agents that take a task and produce a pull request. Several now support running multiple instances in parallel.

Farik should not compete with these. It should sit on top of them. The specification already makes this choice: the developer role delegates to a coding agent runtime rather than reimplementing one. The positioning follows: Farik is the team and the process; the coding agent is the engineer's hands. If the underlying agents get better, Farik gets better. This is the strongest structural position available and it should be stated plainly in the README.

### Agent workforce products

Relevance AI markets an "AI workforce" of named agents. Lindy sells configurable assistants for business workflows. Both lean on the same emotional hook as Farik, agents as colleagues with names, and both are aimed at sales, support, and operations rather than software.

Their existence is evidence that the "hire an agent" framing sells. Their focus elsewhere means there is room.

### Pixel-art agent worlds

Generative Agents (Park et al., 2023, UIST) put 25 agents in a Sims-like town and showed that memory, reflection, and planning produce believable social behavior. a16z's AI Town turned that paper into an open-source pixel-art playground. Gather uses a pixel-art office for human remote teams and proved that people will tolerate, even enjoy, a 16-bit workplace for real work.

None of these do work. AI Town is a toy on purpose; Gather is for humans. But together they de-risk the aesthetic choice: pixel agents in an office reads as charming rather than childish, and the Generative Agents memory architecture is a well-understood template for making the channel feel alive.

### What this adds up to

The intersection of "role-based team", "keeps working on a real repository", "governance as code", and "a UI a founder would open" is empty. That is either a gap or a sign nobody wants it. The MetaGPT and CrewAI adoption numbers and the growth of background coding agents suggest a gap. The counterargument is in the next section.

## Where the idea is weak

Four things could sink it. They are listed in order of how much they worry me.

**Multi-agent teams may not beat one strong agent with a good harness.** There is evidence in both directions and the balance has shifted with each model generation. Anthropic's own guidance for building agents leans toward the simplest structure that works, and single-agent systems with good tools and clear task specs keep matching multi-agent setups on coding benchmarks at lower cost. If a single Claude Code session with a well-written task beats a Farik team of five on the same task, the roles are theater. Farik's answer is that the roles exist to produce the task spec and to verify the result, not to write the code, and that this is exactly the part single agents skip. But that is a hypothesis. Milestone 0 in the specification exists to test it before the office is drawn, and if a PM-plus-developer pair does not produce measurably better-contracted, better-verified work than a lone developer agent, the project should change shape.

**Cost.** Five agents doing real work burn tokens fast. A rough figure: a developer session on a mid-sized task might read 300k to 500k input tokens across turns and write 20k to 40k. At Claude Opus 5 list prices ($5 per million input, $25 per million output), that is somewhere between $2 and $4 per session before caching, and a task often takes two sessions plus a verification session. A sprint of ten tasks lands in the $50 to $100 range. Prompt caching cuts input cost substantially when the system prompt and memory are stable, and the spec's budget defaults are chosen to keep a first day under twenty dollars, but the user is paying for the process overhead of PM and reviewer sessions on top of the work. If the overhead does not visibly buy fewer wasted tasks, users will feel it in their bill first. The cost report per task in the spec is there so that the value is visible next to the cost.

**The channel could be pure theater.** Agents chatting "in a human way" is delightful for a day and then either becomes noise or becomes expensive. The spec rate-limits ambient chatter and forbids the channel from creating work. Whether the remaining conversation is worth its tokens is an open question that only retention data will answer. The uncomfortable possibility is that the right amount of agent-to-agent small talk is zero, and that what users actually want from the channel is a well-written standup summary.

**Platform providers might ship this.** Anthropic has Managed Agents with multi-agent sessions, OpenAI and Google both have agent platforms, and any of them could add a "team" layer with named roles. The defense is that Farik's governance layer is open source and provider-independent in principle, and that providers have so far built primitives rather than opinionated process. That defense is weaker than it sounds; opinionated products get built on primitives all the time, sometimes by the primitive's owner. The realistic mitigation is speed and community: be the open-source default for "agent team with governance" before a platform decides to be.

A fifth, smaller worry: the pixel aesthetic may read as a toy to the second persona's co-founder or investor. The mitigation in the spec is that the office is optional and the board is the primary surface. That should be kept true.

## What Farik does differently

Three things, and the order matters for messaging.

First, governance you can read. The transition table, the definitions of ready and done, the budgets, and the permission tiers are all in a package with no I/O that anyone can audit and that is Apache 2.0 forever. Competing products have a system prompt where Farik has a state machine.

Second, a team that persists. Agents have memory, decisions accumulate, retros feed forward. The project's `.farik/` directory is the team's institutional knowledge and it travels with the repository.

Third, the office. It is the reason someone screenshots Farik and posts it. It is also honest UI: an agent at the whiteboard is planning, at the meeting table is in a ceremony, at their desk is working, and the door is where the human comes in.

The temptation will be to lead with the third. Lead with the first, show the third.

## Business model

Open core, one repository, Apache 2.0 for everything that governs agents and a commercial license for a separate `ee/` directory. This is the GitLab and PostHog pattern. It was chosen over AGPL (Cal.com's route) because the target community is builders who will embed Farik's core in their own tools, and AGPL would make some of them hesitate. It was chosen over a source-available license like n8n's Sustainable Use License because the governance layer's credibility depends on it being unambiguously open.

The free tier is complete. Every role, the office, MCP, skills, the audit log, local execution on the user's key. This is not generosity; it is the acquisition channel, and a crippled free tier would kill the community that the whole plan depends on.

Premium is hosted execution with included credits, cloud sync, long-history analytics, cosmetic packs, and support. Hosted execution is the real product: it removes the API key, the Docker requirement, and the "my laptop was closed" problem in one stroke. Pricing should be a base subscription plus usage, because the cost is dominated by tokens and a flat fee either loses money on heavy users or overcharges light ones. A reasonable starting point is a base fee that includes a credit allowance, with overage at a modest markup on list model prices. The exact numbers should wait for Milestone 0's cost measurements; guessing them now would be guessing.

Cosmetics are worth more than they look. Gather, and every game with a pixel aesthetic, has shown people pay for avatars and themes. It is also the one premium feature with near-zero marginal cost.

Cost structure for the hosted tier is model tokens first by a wide margin, then sandbox compute, then storage. Margin depends on the markup over token cost and on how much caching and cheaper-model routing the harness can do on the user's behalf. This is another reason the harness matters commercially: a governor that stops runaway sessions is also the thing that protects gross margin.

## Go to market

Launch the open-source core when Milestone 2 in the spec is done, not before. A half-working harness in public would undermine the one claim the product rests on.

The launch artifact is a recording, not a landing page: a real repository, a team of five, a sprint from planning to accepted tasks, with the event log and the cost report shown at the end. If the recording is not convincing, the product is not ready.

Channels, in order of expected yield: the communities around coding agents (Claude Code, Cursor, OpenHands users), Hacker News and the indie hacker community, and the MCP ecosystem, where each agent's per-agent MCP configuration is a natural reason for server authors to mention Farik.

Premium follows only after the open-source version has shown retention. If people do not come back to a free Farik, a paid one will not fix that.

## What to measure

Five numbers, tracked from Milestone 0 onward.

- First-pass acceptance rate: tasks accepted on the first verification, without a rejection cycle. This is the quality of contracts and of the developer, together.
- Human intervention rate: escalations per accepted task. Falling over the life of a project means the team is learning; flat means it is not.
- Cost per accepted task, and the share of it spent on PM and review sessions versus the work session. This is the number that decides whether the process overhead is worth it.
- Share of exit criteria verified by command or test rather than by review or human. Higher is better; it means the PM is writing checkable contracts.
- Weekly returning projects. Not users, projects: a team that is still working on the same repository four weeks later is the retention that matters.

## Decisions needed now

1. Confirm the choice to build the developer role on an existing coding agent runtime rather than a custom one. The spec assumes yes. Reversing it later is expensive.
2. Confirm Apache 2.0 with an `ee/` directory. Changing a license after launch is worse than picking a slightly wrong one before.
3. Agree that Milestone 0 is a go/no-go gate on the multi-agent thesis and that a failing result changes the product, not the milestone.
4. Decide whether the first release ships a no-sandbox mode. It will be the most-used mode on Windows if it exists and the source of the first "Farik deleted my files" issue if it is not governed tightly.

## What this analysis did not do

No user interviews were run; the personas are inferred from who uses adjacent tools. No competitor was tested hands-on for this document; the descriptions are from their published materials and papers as of mid-2026. No cost figures were measured; the per-task estimate is arithmetic on list prices and assumed token counts and could be off by a factor of two in either direction. Each of these is a task for the weeks before Milestone 0 rather than a reason to delay it.
