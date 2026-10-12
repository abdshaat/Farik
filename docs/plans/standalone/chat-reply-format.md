# Standalone: an agent's chat reply, summed up and formatted

Status: done
Branch: `fix/chat-reply-format` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.3 (asking an agent a question), F8 (the one-to-one chat in the web app)
Depends on: none (`main` at `9e3d4f3c`)
Readiness confirmed by: a fresh Opus 5.5 session, 2026-10-12 (one round, against `docs/standards/workflow.md` stage 2; ready with findings 1 to 13, carried into this text)

## Goal

The founder, 2026-10-12: "When I talk to agents, they respond in an unformatted .md file in chat. Please change that to make agents respond to chats in the following convention. Hide all the file paths. Structure your response as a summary -> bullet points -> conclusion. The agent may send back a formatted .md file or html file if needed." Today the chat session's closing (`prompt.rs`, `SessionPurpose::Chat`) says nothing about form, so agents write Markdown full of paths, and `OneToOne.tsx` shows it as raw text (`renderMessageText` in a `<p>` with `white-space: pre-wrap`). When done: a chat session is told to answer in Markdown as a short summary, then bullet points, then a one-line conclusion, naming files and folders in plain words and never by their path; and the one-to-one chat shows an agent's reply rendered as Markdown (headings, lists, bold, tables, code and `mermaid` diagrams formatted, raw HTML off), its task ids still linking to their task. Out of scope: the channel's posts (`Conversation` sessions) and its rendering; the interview's form (`INTERVIEW_INSTRUCTION`, one question at a time, which a summary-bullets-conclusion form does not fit; it gains the path sentence alone, and its replies are rendered as Markdown like every agent reply); the chat list's last line; HTML in a reply (see Decisions).

## Decisions

- The form is asked for in the chat's closing, `CLOSING_INSTRUCTIONS`' `SessionPurpose::Chat` entry, the one text every non-interview chat session ends with. Rejected: the tool's `text` description (the closing is where each purpose's instructions live; one place).
- The new closing, exactly: "This session answers the user in your one-to-one chat: the newest of their messages in `From the human`, with the chat before it. Answer once with `catervas_chat_reply`, in your persona's voice, in Markdown: a summary of one or two sentences, then the points as a bulleted list, then a one-line conclusion; a greeting or a one-line answer needs no list. Never write a file's or a folder's path: name it in plain words, as the user knows it. When a table or a diagram says it better, put it in the reply: Markdown tables and `mermaid` diagrams are shown formatted. You can read the project and change nothing. When work is needed, put a request in your reply, a title and what it asks for, for the user to send. Then end the session." Rejected: forcing the list on a greeting (a list under "Hello" reads as a form, not a teammate).
- The interview is a chat too: `INTERVIEW_INSTRUCTION` gains the same sentence, "Never write a file's or a folder's path: name it in plain words, as the user knows it.", placed before "Answer once". Nothing else pins its text (`session.rs:3477` checks only that the prompt ends with it).
- "Hide all the file paths" is held by the prompt alone. Rejected: stripping paths from the text in the runtime or the UI (a heuristic that would eat real words such as "and/or" and URLs, for a rule the model follows).
- "A formatted .md file" is the reply itself, rendered as Markdown in the chat. Rejected: an attachment field on `catervas_chat_reply` (a new event field, query and view for what the rendered reply already shows; a chat session is read-only and writes no file). HTML is not rendered: an agent's text is untrusted (`MessageText.tsx`), and the existing `Markdown` component keeps raw HTML off. HTML documents are the Files page's (draft pull request #47). To confirm with the founder in the pull request description, as an assumption.
- An agent's message (author not `human`) in `OneToOne` is shown with the existing `Markdown` component (`components/Markdown.tsx`) in a `<div className={channel.text}>` (a `<div>`, since Markdown makes blocks, which a `<p>` cannot hold), without `styles.lines` (`Chats.module.css`'s `.lines`). Single line breaks in an agent's reply fold as standard Markdown (`breaks: false`, the shared instance unchanged); the closing asks for Markdown, so paragraphs and lists carry the layout. The user's own message keeps `renderMessageText` in its `<p>`, unchanged. Rejected: a second Markdown renderer or a markdown-it plugin for mentions (a one-to-one has no one else to mention); an agent's `@` mentions in a one-to-one show as written.
- Task ids stay links: before rendering, `linkTasks(text)` replaces, in one pass of ``/(```[\s\S]*?```|`[^`\n]*`|\[[^\]]*\]\([^)]*\))|\bCTV-[0-9]{1,6}\b/g``, each bare task id with `[CTV-n](/tasks/CTV-n)`, and leaves group 1 (a fenced block, a code span, a whole link) unchanged. `// ponytail:` comment: `~~~` fences and indented code are not skipped; a full Markdown parse if that ever bites.
- A click in the reply's `<div>` with `button === 0` and no Ctrl, Meta, Shift or Alt key, whose target's closest `a` has an `href` matching `^/tasks/CTV-[0-9]{1,6}$`, calls `event.preventDefault()` and `navigate(href)`; every other link is left to the browser. The `<div>` carries `// biome-ignore lint/a11y/useKeyWithClickEvents lint/a11y/noStaticElementInteractions: the links inside are the interactive elements and keep their keyboard behaviour`.
- Tables and the reply's edges: `Markdown.module.css` gains `.text table { display: block; overflow-x: auto; border-collapse: collapse; margin: 0 0 var(--catervas-space-3); }`, `.text th, .text td { border: 1px solid var(--catervas-color-rule); padding: 4px 8px; text-align: left; }` and `.text > :last-child { margin-bottom: 0; }` (no gap above the proposal box or "What I heard"); the Files page gains the same, which it lacked. Rejected: a full page load on each task link (drops the page's state and the live connection).
- Spec revision 0.107 (renumbered at merge if `main` takes it first): a header line at `docs/SPEC.md` after 0.106; 4.3 gains one sentence on the reply's form; F8's "and the agent's text shown as text" becomes "and the agent's text rendered as Markdown (raw HTML off, unsafe links left as text), its task ids linked". No new event, command or query.

## File map

```
crates/runtime/src/prompt.rs                     modifies, tests (Task 1)  the chat's closing
apps/web/src/pages/OneToOne.tsx                  modifies        (Task 2)  an agent's reply as Markdown, task links
apps/web/src/pages/Chats.test.tsx                tests           (Task 2)
apps/web/src/components/Markdown.module.css      modifies        (Task 2)  tables, the last block's margin
apps/web/src/components/Markdown.test.tsx        tests           (Task 2)
docs/SPEC.md                                     modifies        (Task 3)
docs/plans/standalone/chat-reply-format.md       modifies        (each task) its checkbox
```

## Interfaces

Consumes: `CLOSING_INSTRUCTIONS`, `closing` (`prompt.rs`, on main); `Markdown` (`components/Markdown.tsx`, on main); `useNavigate` (`react-router`); the `chat(...)` and `renderApp` helpers of `Chats.test.tsx`.

Produces:

```ts
// pages/OneToOne.tsx (module-private)
function linkTasks(text: string): string;
```

## Tasks

### Task 1: the chat's closing asks for the form

Files: modified and tested `crates/runtime/src/prompt.rs` (its `mod tests`): the chat's closing and `INTERVIEW_INSTRUCTION`

Tests, each watched to fail before the closing changes:

- `chat_closing_asks_for_a_summary_points_and_a_conclusion` — `closing(SessionPurpose::Chat)` contains "in Markdown", "a summary of one or two sentences", "a bulleted list", "a one-line conclusion" and "Never write a file's or a folder's path", in that order of appearance for the first four; `closing(SessionPurpose::Chat)` and `INTERVIEW_INSTRUCTION` both contain "Never write a file's or a folder's path: name it in plain words, as the user knows it."
- the existing closing tests stay green (the chat closing still names `catervas_chat_reply`, the request, and ending the session).

- [x] `fix(runtime): ask a chat's reply for a summary, points and a conclusion, without paths`

### Task 2: the chat shows an agent's reply formatted

Files: modified `apps/web/src/pages/OneToOne.tsx`, tested by `apps/web/src/pages/Chats.test.tsx`

Files: also modified `apps/web/src/components/Markdown.module.css`, tested by `Markdown.test.tsx`

Order: write `renders an agent's reply as Markdown`, watch it fail, render agent messages with `Markdown`. Then write `links a task id in an agent's reply`, watch it fail (no link named CTV-7), add `linkTasks`; watch its click half fail (the route does not change), add the `onClick`. Then `leaves task ids in code and links alone`, then the table.

- `renders an agent's reply as Markdown` — an agent message `"Short answer.\n\n- **One** point\n- Two\n\nThat is all."` shows a `list` with two `listitem`s, "One" inside a `<strong>`, and no literal `**` or `- ` in the message's text.
- `links a task id in an agent's reply` — an agent message `"CTV-7 is in review."` has a link named "CTV-7" with `href` `/tasks/CTV-7`; clicking it moves the app's `MemoryRouter` to the task page: `contract.get` is asked with params `{ task_id: "CTV-7" }` (`TaskDetail.tsx:147`) and the heading "Talking with Mira" is gone.
- `leaves task ids in code and links alone` — an agent message ``"`CTV-7` and [CTV-8](/tasks/CTV-8)"``: `CTV-7` is inside a `code` element and is no link; exactly one link is named "CTV-8", its `href` `/tasks/CTV-8`.
- `keeps the user's own message as text` — a guard that passes from the start, held by mutation (rendering the human's message with `Markdown` makes it fail): a `human` message `"- **not** a list"` shows its text exactly, no `list` role, inside the element carrying `own.lines` (`Chats.module.css`'s `.lines`, as `opens_a_one_to_one` reads it).
- `renders a table` (`Markdown.test.tsx`) — `"| a | b |\n|---|---|\n| 1 | 2 |"` shows a `table` with two `columnheader`s.
- the existing `renders_agent_text_as_text` stays green (markdown-it escapes it); `passes_axe_on_the_chats`'s open chat gets one more message, `chat("mira", "Short answer.\n\n- **One**\n- Two\n\nCTV-12 is next.")`, and stays green; every other existing test stays green.

- [x] `fix(web): show an agent's chat reply as Markdown`

### Task 3: the spec

Files: modified `docs/SPEC.md` (header 0.107, 4.3, F8, as Decisions says)

- [x] `docs(spec): an agent's chat reply is summed up, without paths, and formatted`

## Verification

```
cargo xtask check
# expected: xtask check: ok (format, clippy, tests, bare-TODO, core no-I/O, and the front end's pnpm check)
```
