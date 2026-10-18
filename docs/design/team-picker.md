# Choosing a team, and one agent alone

Status: decided by the planner (Opus 5.5), 2026-10-18, on the founder's brief of that day; the founder answers no questions on it ("I will not be around so dont ask me any questions"). ADR 0071. SPEC revision 0.125.

## The brief

The founder, 2026-10-18, verbatim where quoted:
1. "Choosing a team has a very Bad UI/UX. I need it to be better by suggesting a team based on the folder he chooses."
2. "Show the user the Suggested teams as cards and then the user may choose to have a single customizable agent."
3. "The design must be kept as simple as apple design while at the same time following Catervas brandkit."
4. "Make the user able to choose the specific model for an agent, temp, and the effort level rather than 'Strongest model, think hard'. I need the user to be able to know that this is opus 5.5 on medium effort and this is the temp. which all are customizable per agent."
5. Asked mid-work what a single agent is: "A single agent may operate outside any team function." And: "This is a change in the current Catervas product where we introduced a single customizable agents if the user chooses to."

## What was wrong (the screen before)

Setup's "Your team" (step 5 of 8) opened on a four-way radio list (the suggested six, a ready-made team, a saved team, from scratch), each radio opening a second radio list, then a column of rows: a tick, a picture, a role over a text field, a paragraph per role, then a "More roles" section with its own note. Every agent's model was hidden until after setup, where the Team page said "Strongest model, thinks hard" and the editor offered "Quick / Balanced / Careful". The folder the owner had just picked played no part: a shop's folder and an app's folder were offered the same software team first.

## What the owner sees

### 1. A team picked for the folder

The step's title is "Your team". Its lead is one line that says why the first card is first, from what Catervas found in the folder, for example "Picked for a TypeScript app with tests." or "Picked for a folder of spreadsheets."

Under it, cards, one per way to start, each a button that selects it:
- **The suggested card**, full width, first: the team the folder fits best, its faces in a row (the screen's one bold element: the team is the interface), its name, one line on what it does, and a small "Suggested" mark.
- **The other ready-made teams**, in a grid of smaller cards with the same parts.
- **One agent**: one face, "One agent", "Works on its own. You give it each job." (section 3).
- **Your saved teams**, when there are any, as cards of their own after the ready-made ones.
- **Build your own** is the last card: a Product Manager and a Developer, to which the owner adds.

The old "suggested six" start is gone: "Software product" gains the UI/UX Designer and is the team for a folder of code; the Scrum Master, whose sprints deliveries replaced (ADR 0062), is one "Add someone" away (section 5). Selecting a card fills "Who's on it" below at once; nothing asks a second radio question.

### 2. Who's on it, and each agent's model

"Who's on it" is one grouped list on a raised surface, a row per agent: the picture in its role's ring, the name over the role, and on the right the model and effort in their own two lines, "Opus 5.5" over "High effort". A row is a button that opens the agent's sheet. "Add someone" is the list's last row. An agent this computer cannot run (the Designer without Docker) shows "Needs Docker" in place of its model and opens a sheet that says how to get it.

**The agent's sheet** (a dialog on a desktop, full screen on a phone):
- the picture, large, and the name in a text field;
- **Job**, the role: fixed for a team's agent (shown as text), a choice for one agent alone;
- **Model**, a segmented control of the newest model of each family the price table knows, named as Anthropic names them: "Fable 5.1", "Opus 5.5", "Sonnet 5.5", "Haiku 5.5". Its ⓘ says, in bullets, what each is for and that the dearer ones cost more per day;
- **Effort**, a segmented control of the five levels Claude Code accepts: "Low", "Medium", "High", "Extra high", "Max". Its ⓘ says, in bullets, that more effort thinks longer and costs more;
- **Temperature**, a row that reads "Set by the model", with an ⓘ: "Claude 5 models choose their own temperature. Catervas can't change it." (section 4);
- **What it focuses on**, the persona, in a text box;
- **On the team**, a switch (absent for one agent and for the Product Manager, who every team needs);
- for the Designer, how to open the app (the preview fields that sat on its row before).
"Done" closes the sheet; every change is in the draft at once, as typing in the old rows was.

The same model, effort and temperature rows replace "How carefully … works" and the model menu on the Team page's agent editor, and every place that named a model in words ("Strongest model, thinks hard", "the strongest model") names it as "Opus 5.5" and its effort as "High effort".

### 3. One agent, alone

A project may have one agent instead of a team. It works outside every team function:
- **No Product Manager, no reviewer, no planning.** The owner's request goes to it as a task it is given (ADR 0063's given task): it writes its own contract, Catervas assigns it at once, and it does the work. Its code is not reviewed by an agent; the owner reads it in the pull request, which Catervas opens and never merges (ADR 0061).
- **Any job.** The owner chooses its job from the roles Catervas has: by default the Developer for a folder of code and the Finance Specialist for a folder of spreadsheets. Its tools, skills and connectors are that role's kit.
- **Customizable** like any agent: name, picture, model, effort, what it focuses on.
- **What is hidden.** No deliveries, no Catervafication offer, no "Add someone" on the Team page (turning one agent into a team is "Use a saved team" or a new setup). Chats with it are as with any agent.

How it works is in section 6, after the runtime's paths were mapped.

### 4. Temperature

The owner asked to choose each agent's temperature. Catervas cannot, and the sheet says so rather than offer a control that does nothing:
- Agents run in Claude Code, whose command line takes a model and an effort and no temperature.
- Every model Catervas offers (Fable 5.1, Opus 5.5, Sonnet 5.5, Haiku 5.5) refuses a temperature other than its own default (the API answers 400).
So the row shows the fact, and the control waits for an engine and a model that take one (phase 12, engines and providers).

### 5. Suggesting a team from the folder

`team.propose` answers, beside the suggested agents, `suggested_teams`: every ready-made team's slug in the order the folder fits them, each with the reason its card shows. A pure function in `catervas-core` orders them from the folder's tracked paths:
- **Code** (a language the scan knows, or a manifest such as `package.json` or `Cargo.toml`): Software product first, "Picked for a <language> app" and "with tests" when the scan found tests.
- **Money** (spreadsheets and ledgers: `.csv`, `.xlsx`, `.xls`, `.ods`, `.numbers`, `.qbo`, `.ofx`, `.beancount`, `.ledger`, or names with ledger, budget, invoice, portfolio, finance): Financial analysis first, "Picked for a folder of spreadsheets".
- **A shop** (a Shopify theme's `.liquid` files, or names with product, inventory, catalog, shop, store, order): Online store first, "Picked for a shop's files".
- Several signals: the one with the most files wins; code wins a tie, since the project is a git repository.
- Nothing tracked yet: Software product first, "Picked for a new project".

The "Software product" team gains the UI/UX Designer (Iris), unticked on a computer without Docker as before.

## 6. How one agent works

A team file says it is one agent alone with `policy.solo: true`. A team of one Product Manager is not solo by inference: the flag is explicit, so nothing a team does changes for a team that happens to be small.

**Validation** (`validate_team`):
- `solo: true` holds exactly one agent that is not retired, of any role but the Scrum Master (who has nothing to run alone) and the UI/UX Designer (whose plans wait for a Product Manager's approval); `REQUIRED_ROLES` does not apply. Refused otherwise with `solo_one_agent` or `solo_role`.
- Without it, a team still needs an active Product Manager and two agents; the schema's `minItems` becomes 1 so the solo file validates, and `validate_team` keeps the two for a team.
- Pausing the one agent is allowed; retiring it is refused (`leaves_a_gap`), as the last Product Manager's is.

**A request becomes the agent's task.** In solo, `request.file` (Today's box, the ask dialog) posts the owner's words to the agent's chat as "Send as a task" does (`chat_message_post` with `as_task`), so it is a given task (ADR 0063): auto-triaged small, linked to its chat message, `given_to` the agent. Its 20-character floor stays.

**The given task runs without a Product Manager.** Each place that waits for one takes the given agent instead, in solo only:
- draft to refining (rule 10, `requests.rs` `draft`): moved by the given agent, which holds the Product Manager's standing on its given task while `draft` or `refining` already;
- the product plan's wait (`awaits_the_product_plan`): does not apply;
- readiness: `ReviewerAvailable` and the private folder's reviewer rule do not apply; the contract's `reviewer_role` stays its usual one, unread;
- ready to assigned (rule 8, `ready_given`): Catervas assigns it to the given agent with no assigner and no reviewer, the given agent's standing extended to that one move;
- review: `reviewed_by_an_agent` is false in solo, so Catervas verifies the contract's criteria alone (`catervas_verifies_alone`) and accepts it; the pull request it then opens is the owner's review, and Catervas never merges (ADR 0061).

**What does not run in solo:** epics and their breakdown, deliveries and their review and retro, the plan session for a non-given ready task (there is none), the product-plan interview, the Catervafication (offer hidden, start refused), the document refresh. Each already skips without a Product Manager; where the one agent is a Product Manager, a solo check skips it too.

**Chats** work as with any agent; "Send as a task" and `catervas_take_task` file the same given task.

**On the web:** the Team page shows the one agent's card and editor, without "Add someone" or "Use a saved team"; Today's band holds one face.

## Out of scope

A temperature control (section 4); a "general" role for one agent (a role is a role file, a kit and a persona: one agent uses an existing role); turning one agent into a team in place; the Team page's own look beyond the agent editor's model rows.
