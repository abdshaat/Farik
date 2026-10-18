# Standalone catervafication-work, plan 5: the web app

Status: done
Branch: `feat/catervafication-work` (work outside a phase; see `README.md`)
Spec: `docs/SPEC.md` 4.1 (the offer), 4.3, F8
Depends on: plans 1 to 4 of this branch (3b and 4b included)
Readiness confirmed by: Opus 5.5 readiness review, 2026-10-16

Signatures, not bodies; test names and what each asserts, not test code (ADR 0008).

## Goal

In a one-to-one chat, the box has "Send as a task" beside Send, with an info button saying what it
does. A message sent that way shows "Given to <Name> as <id>", linking to the task, and the chat
shows no "thinking" for it. The two refusals read in plain words. The end-of-setup offer and the
quiet card say the team also hunts for bugs and starts the marketing. The task page's activity
names a connection request. Out of scope: any change to "Send as a request" under an agent's
proposal.

## Decisions

- **The button.** "Send as a task" is a `Button` of kind `secondary`, `type="button"`, after Send
  and inside the composer's row. It sends what Send sends, with `asTask: true` in the body:
  `client.command({ command: "chat_message_post", body: { agentId, text, asTask: true } })`.
  - The 4,000-character check and the refusal paragraph are Send's.
  - It is `busy` while the call runs. On success the box empties, as for Send.
  - Enter still sends a plain message; nothing types a task by accident.
  - The wire body is `{ agent_id, text, as_task: true }`, through the client's one mapping layer.
    A plain Send sends no `as_task`, so the existing test's exact body holds.
- **Its info.** An `InfoTip` with id `chat-task-info` follows the button. Its text is
  `chatsTaskInfo` "{name} turns your message into a task, plans it and starts on it. Changes to
  code are still checked before they reach you." This follows the founder's rule: short words on
  screen, the detail behind the info button.
- **The link.** A message whose author is `human` and whose `sentAs` is set shows, under its text,
  a `Link` to `/tasks/<sentAs>` whose text is `chatsGivenAs` "Given to {name} as {id}". An agent's
  proposal keeps its own "Sent as" inside `ProposalBox`, unchanged.
- **Refusals in words.** `refusals.ts` maps two codes:
  - `agent_not_active` → `refuseAgentNotActive` "This teammate is paused. Resume them on the
    Team page to give them a task.";
  - `task_too_short` → `refuseTaskTooShort` "Say a little more: a task needs at least 20
    characters."

  The composer already shows `sentence(reply.error.detail)`, which reads the map.
- **The Catervafication's words** (`en.ts`; the values after ADR 0062's plan 4 are the base):
  - `readyOfferLead`: "Your team reads your project, writes down what it learns, looks for bugs
    and starts your marketing. You can read all of it in Files."
  - `readyDeveloper`: "writes how your project is built and worked on, and looks for bugs in the
    code."
  - `readyDesigner`: "writes down every screen your project has, and looks for what is broken on
    them."
  - `readyMarketing`: "writes your marketing plan and starts on it; anything that costs money
    waits for you."
  - `readyNote`: "Your team adds its notes and files each bug it finds as a request; nothing in
    your code changes yet."
  - `cardLead`: "A first piece of work where your team reads your project, writes down what it
    learns, looks for bugs and starts your marketing."
- **The activity word.** `TaskDetail.tsx`'s map gains `"connection.requested":
  "toldConnectionRequested"`, `en.ts` `toldConnectionRequested` "Asked you to connect a service."
- **No new ARIA pattern.** The new button and link are named by their text. The page's existing
  axe test covers them once the fixture holds one of each.

## File map

```
apps/web/src/pages/OneToOne.tsx             modifies: Composer's second button and InfoTip (Task 1); the human message's link (Task 2)
apps/web/src/app/refusals.ts                modifies: two codes (Task 1)
apps/web/src/strings/en.ts                  modifies: chatsSendTask, chatsTaskInfo, refuseAgentNotActive, refuseTaskTooShort (Task 1); chatsGivenAs (Task 2); the Catervafication words, toldConnectionRequested (Task 3)
apps/web/src/pages/Chats.test.tsx           tests:    Tasks 1 and 2
apps/web/src/pages/TaskDetail.tsx           modifies: the activity word (Task 3)
apps/web/src/pages/setup/setup.test.tsx     tests:    the offer's lines (Task 3)
```

## Interfaces

Consumes: the wire of plan 4 (`chat_message_post`'s `as_task`, `chat.messages`'s `sent_as` on a
human message, which `chat_messages` already answers from `from_chat_message`) and plan 1
(`connection.requested`); `Button`, `InfoTip` (`@catervas/ui`); `useConnection`, `sentence`, `t`;
`chats()`, `s.calls`, `s.reply` (`Chats.test.tsx`); `offers_the_catervafication_at_the_end_of_setup`'s
set-up (`setup.test.tsx`).

Produces the string keys `chatsSendTask` "Send as a task", `chatsTaskInfo`, `chatsGivenAs`,
`refuseAgentNotActive`, `refuseTaskTooShort` and `toldConnectionRequested`, and the new values
above. There is no new component or export.

## Tasks

### Task 1: Send as a task

Files: `apps/web/src/pages/OneToOne.tsx` (`Composer`), `apps/web/src/app/refusals.ts`,
`apps/web/src/strings/en.ts`; tested by `apps/web/src/pages/Chats.test.tsx`. The client's
`Command` type is generated from `command.schema.json` (plan 4 added `as_task`), so nothing in
`packages/protocol-client` is edited.

- `sends_a_message_as_a_task`, at `/channel/mira`:
  - typing "Add a dark mode switch to the settings page" and pressing "Send as a task" sends one
    `command` with params `{ command: { command: "chat_message_post", body: { agent_id: "mira",
    text: "Add a dark mode switch to the settings page", as_task: true } } }`;
  - answered `{ said: "given to Mira as CTV-7", events: [9, 10, 11] }`, the box is empty;
  - the button "More about this" whose `aria-describedby` is `chat-task-info`, clicked, has
    `aria-expanded` `true`, and the element `#chat-task-info` has the text `chatsTaskInfo` filled
    with "Mira";
  - the next send, answered `{ error: { kind: "refused", detail: "task_too_short: say a little
    more: at least 20 characters" } }`, shows an alert whose text is `en.refuseTaskTooShort`.

- [x] `feat(web): send a chat message to an agent as its task`

### Task 2: the message shows its task

Files: `apps/web/src/pages/OneToOne.tsx` (the message's body), `apps/web/src/strings/en.ts`;
tested by `apps/web/src/pages/Chats.test.tsx`.

- `shows_the_task_a_message_became`. `chat.messages` answers one human message, `sent_as:
  "CTV-7"`, and `waiting: null`. Then:
  - a link named "Given to Mira as CTV-7" has `href` `/tasks/CTV-7`;
  - `en.chatsAnswering` filled with "Mira" is not on the page;
  - `expectNoAxeViolations(container)` passes.

- [x] `feat(web): a chat message given as a task links to its task`

### Task 3: the Catervafication's words and the activity word

Files: `apps/web/src/strings/en.ts`, `apps/web/src/pages/TaskDetail.tsx`; tested by
`apps/web/src/pages/setup/setup.test.tsx`.

- `the_offer_says_the_team_hunts_and_markets`. With the set-up of
  `offers_the_catervafication_at_the_end_of_setup` and `catervafication.get` answering `roles:
  ["product_manager", "software_developer", "ui_ux_designer", "marketing_specialist"]`, the
  offer's text holds the new `readyOfferLead`, and each of the new `readyDeveloper`,
  `readyDesigner` and `readyMarketing` after its agent's name.

Update `offers_the_catervafication_at_the_end_of_setup`'s expected words to the new values.

- [x] `feat(web): the offer says the team hunts for bugs and starts the marketing`

## Verification

```
pnpm check
# expected: exit 0, every check passing
cargo xtask check
# expected: xtask check: ok
```
