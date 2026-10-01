# 0026. A UI/UX Designer, chats and team templates in phase 6

Date: 2026-09-30
Status: accepted. The founder approved it on 2026-09-30, with one change: the Designer checks a Developer's UI change in the browser before it goes to the Architect for approval (A4). Amended 2026-09-30 (project plan revision 23), on step 11's readiness review: step 11 is split in two, 11 the Designer and its plan gate, 12 the preview, the connector and the design review; chats become step 13, templates step 14, and the milestone runbook step 15. Amended 2026-10-01 (project plan revision 25, ADR 0028): a new step 15, sprints gather ready work, comes before the milestone runbook, which becomes step 16. The founder also decided that the preview runs as a `prepare` and a `start` command, both in the sandbox, and that the Designer works only with Docker's sandbox on, its browser reaching only the preview. The step numbers below are the ones before the split.

## Context

Phase 6 landed steps 01 to 10 on 2026-09-30, and step 11, the Milestone 0 and 1 runs, waits on the founder. Three small fixes landed first: the Ready pill on the computer check, the waiting rows' buttons, and the rail's breathing Connected dot. Before the runs, the founder decided on 2026-09-30 that three more things belong in the product the Milestone 1 users test:
- **A UI/UX Designer.** Farik is for non-technical users (ADR 0016), and they judge a product by its interface. No role owns the interface today: the Developer builds screens as a side effect, and the Architect reviews code, not pages.
- **One-to-one chats.** A user can talk to one agent only through the team channel, in public. Spec 4.3's read-only one-on-one was planned for phase 8 step 04.
- **Team templates.** A tuned team is rebuilt by hand for every project.

Three constraints shaped the design:
- **Only the Developer changes code.** The founder decided this on 2026-09-24, and spec 0.11 enforces it through `document_paths` (5.3, 5.12). A Designer that only wrote documents could not change a screen.
- **No MCP server but Farik's own.** Until phase 8, a session gets Farik's server and the hook denies every other server's tool (5.6). A Designer who cannot open the app cannot explore it.
- **Chat is not command** (5.1). Nothing said may create work, and a one-on-one must change nothing (4.3).

The questions, the options, and the choice for each. Where the founder did not rule, the design chose, and the choice is marked "design".

**A1. How the interface gets an owner.**
- A skill for the Developer. Cheap, but nobody would look at the running app as a user does, or check the Developer's screens.
- A kit for the Developer in phase 9. Too late for the milestone, and the author still reviews its own screens.
- A new role. **The founder's choice.** Its wire id is `ui_ux_designer`. It joins the suggested team, which grows from five to six (PM, Scrum Master, Architect, Developer, UI/UX Designer, Marketing). The user can untick it, and the cap stays seven.

**A2. What the Designer may change.**
- Documents only, like the Marketing Specialist. It could then plan changes but never make them.
- Any code, like the Developer, on the tasks assigned to it. **The founder's choice.** "Only the Software Developer changes code" becomes "only the Developer and the UI/UX Designer change code". This amends the founder's decision of 2026-09-24. Design: the team's `permissions` answers apply to the Designer as to the Developer.

**A3. The Designer's flow.**
- Implement directly, as the Developer does.
- Explore first. **The founder's choice.** The Designer explores the current UI in the browser (Playwright), plans its changes, and sends the plan to the Product Manager. The PM must approve it before implementation. This is a new gate, and the human's existing approvals for epics and high-risk work still apply. Only then does it implement.
- Design: the plan gate adds events (`design_plan.proposed`, `.approved`, `.returned`), not lifecycle states. The governor denies the Designer's write, execute and commit tools until the plan is approved. Returned plans count against the task's try limit.

**A4. Reviews.**
- The Architect reviews the Designer's work, because the author never accepts its own. **The founder's choice.** Design: with no active Architect, a Developer reviews it.
- The Designer checks every UI change the Developer makes, in the browser, before the Architect sees it: at phone and desktop sizes, in both themes, with an accessibility check, passing it on or sending it back to the Developer with reasons. Only a change the Designer passed goes to the Architect for approval. **The founder's choice, in his words of 2026-09-30:** "The UI designer checks it in the browser before sending it to the architect for approval." Design: the contract still names the Architect as reviewer; the Designer's pass is the event `design_review.recorded`, which gates the start of the Architect's review, and no state is added. The design review is in addition to the code review, not a replacement, so a mixed change gets both, the Designer's first. It applies when the team has a Designer who is not retired; without one, the Architect reviews alone, as before.
- What counts as a UI change. Paths alone would miss wording changes. A contract field alone would trust the author to remember. Overlapping `allowed_paths` with UI globs at readiness cannot be computed exactly. Design: both. A Developer's task is a UI change when its diff touches the new team rule `ui_paths`, judged by the governor at `verifying`, or when its contract sets the new field `ui_change`.

**A5. The Designer's skills.** **The founder's choice**, in the Agent Skills format: UX review heuristics, WCAG 2.2 accessibility checks, the project's brand and design tokens, plain-language interface wording, writing mockups, and responsive and phone-width checks.

**A6. How the Designer reaches a browser.**
- Wait for phase 8's MCP per agent. The Designer would ship blind.
- Build phase 8 step 01 whole now: custom servers, user tagging, credentials, approvals. Too much for a milestone phase.
- "Just enough, built to grow". **The founder's choice:**
  - per-agent connectors in the team file;
  - the governor sees and checks every connector tool call, with its tier and tag;
  - one built-in connector, Playwright, limited to browsing the project's own preview;
  - Farik starts the preview from a "preview" command the user sets once in Settings, which for Farik itself starts Farik's own web app;
  - browsing is limited to that preview address;
  - it is engine-neutral (MCP), in line with ADR 0023, and phase 8 later extends the same base with credentials, allowances and other connectors rather than replacing it.

  Design: the preview and the browser run in containers, the browser in the preview's network namespace, and the governor checks every `url` argument against the preview's origin. The accessibility check is a Farik tool that runs axe-core itself, and the agent is never given a tool that runs script in the page.

**A7. The Designer's character and tag colour.** Open. The founder chooses them in the step 11 mockups.

**B. Chats.**
- Mentions in the team channel, as today. Public, and every agent reads them.
- A chat list. **The founder's choice.** The Channel page becomes a list: Team (today's channel), plus one private one-to-one chat per agent. A chat follows 4.3's read-only rule: the agent answers from its memory and read access to the project, and changes nothing. When work is needed, its reply carries a "Send as a request" button, and nothing is filed without the human. Chats are private: not posted to the channel and not seen by other agents. History is kept. An agent answers even while the team is paused. Chat cost shows on the Costs page as "Conversations". This pulls one-on-ones forward from phase 8, and replaces phase 8's plan that the agent files a `draft` itself.
- How a chat is recorded. Design: a kind of its own, `chat_message.posted`, so nothing that reads the channel reads it. A filed proposal is linked through `request.file`'s new `from_chat_message`.
- How a chat stays read-only. Design: a `chat` session gets the read tier alone, whatever the agent's grants. It has the reading built-ins in the project root, Farik's five reading tools, and `farik_chat_reply`, and no connector, web tool, memory write, channel post or task creation. The hook denies anything else. It runs on the agent's own model at `low` effort, counts toward the daily budget, and a paused agent answers too.

**C. Team templates.**
- Per project. Useless across projects, which is the point.
- Machine-local, in Farik's state folder, available to every project. **The founder's choice.** A template holds each agent's role, name, picture, model and effort, and the team's rules: permissions, plan check, spending, and finishing work. It holds nothing project-specific (paths, checks). Setup's "Your team" step offers three starts: the suggested team, a saved team, or from scratch. The Team page gets "Save as a template" and "Use a saved team". Switching keeps the safety rules: worked agents are retired, not deleted; a Product Manager and a Developer are required; the cap is seven. "One team per project" stays true: templates are reuse, not several live teams.
- Design: a template also holds each agent's persona. An agent matching by id and role is kept through a switch, and saving a template records no project event.

**D. Placement.**
- Phase 8 and phase 9, where one-on-ones, connectors and kits were planned. The milestone users would test a product without them.
- Phase 6, before the milestone runs. **The founder's choice.** Step 11 is the UI/UX Designer and the Playwright connector, step 12 chats, and step 13 team templates. The milestone runbook, formerly step 11, becomes step 14. Each step starts with mockups the founder approves before code.

## Decision

Phase 6 gains three steps before its milestone runs:
- **11:** a sixth suggested role, the UI/UX Designer (`ui_ux_designer`). It changes code on its own tasks after exploring the app and having its plan approved by the Product Manager. The Architect reviews the Designer's work. The Designer checks the Developer's interface changes first, in the browser, and only a change it passed goes to the Architect. It uses a built-in Playwright connector confined to the project's preview.
- **12:** private, read-only one-to-one chats beside the team channel.
- **13:** machine-local team templates.

The milestone runbook becomes step 14. `docs/design/designer-chats-templates.md` is the design, and the spec changes land with each step (hard rule 8).

## Consequences

Easier:
- The interface gets an owner who looks at the running app, and every screen a Developer ships is checked at two sizes, in two themes, for accessibility, before the Architect and the user see it.
- The user can ask one agent a question without the team hearing, and turn the answer into work with one button, under the same triage as any request.
- A tuned team is saved once and used in every project.
- Phase 8 starts from a working, governed connector base instead of a blank one, and phase 9's kit format has its first connector and its first drift test already.

Harder:
- Step 14's team sprint runs seven agents, the six of phase 4 and the Designer (the cap).
- Phase 6 grows from eleven steps to fourteen, and the milestone runs wait for three more steps and their mockups.
- The rule "only the Developer changes code" now has two roles in it, and every place that says it changes: spec 5.3, 5.12, 6.1 to 6.5, and the roles' prompts.
- A Designer's task costs more before any code: an exploration, a plan, and the Product Manager's decision. A UI change costs a design review before its code review. A team that finds this slow can untick the Designer.
- The Designer needs Docker and two more images (the preview and the Playwright server), and a preview command that works in the sandbox. A project whose app needs a database or secrets to start will not preview until the user makes that work.
- Farik now ships a third-party MCP server's image, and a pinned tool list that each update must re-tag by hand.
- Chats spend money while the team is paused. The daily budget still bounds them.
- A chat is private from the team, not from the machine: it lives in the event log like everything else.
- The design made choices the founder has not ruled on, each marked "design" above. Any of them the founder changes is changed in the step plan that builds it.

ADR 0020 and ADR 0025 carry a one-line amendment pointing here, and keep their text.
