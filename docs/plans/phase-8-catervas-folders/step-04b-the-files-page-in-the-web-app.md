# Phase 8, step 04b: The Files page in the web app

Status: ready
Branch: `phase/8-catervas-folders` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 4.5 (new); no F-number covers the folders' page
Depends on: step 04 of this phase (planned, not yet executed: `files.tree` with `integration`, `files.read` with `waiting`, `rederiving` and `editable`, `files.save` and their wire and refusal codes, `"files.save"` in `MethodName`), which rests on step 03's folder changes; step 03b (planned, not yet executed: the Today card `DocumentChanges`, which defers its "Open in Files" to this step); phase 7 and the Catervas rename (merged on main, 2c28b555); task ids `CTV-<n>` (merged on main, #33). Nothing of steps 02, 05, 06 or 07.
Mockups approved by: the founder, 2026-10-10 (canvas "Catervas folders", version 19)
Readiness confirmed by: a fresh-session Opus reviewer, 2026-10-10 (one round, ADR 0032): ready; T1–T3 and nits carried into execution, with S3's `file_busy` words from step 04, folded below.

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008). Line numbers are at ac1a606e. Build from `docs/design/mockups/Files.dc.html`, `FilesForYou`, `FilesEdit`, `FilesChanged`, `FilesStale` and `PhoneFiles` (`.dc.html`); their words are this plan's words.

## Goal

The owner opens "Files" in the rail (on a phone, from Team), sees one folder per active role with its owner's face and tag colour, reads a document rendered from Markdown with its Mermaid diagrams drawn, switches a pair between "For you" and "For the team", sees who changed it last, when and in which task, and that the team's version is being brought up to date; and edits a document in plain Markdown with a preview, saving it as their own change, told in words that match the team's integration policy when it reaches the project, their text kept when the file changed meanwhile. Today's document card gains "Open in Files". Out of scope: "A change to this waits for your approval" (`FilesForYou.dc.html`), which reads step 03's `folder_doc.proposed` and is a later step's job, not stubbed here; new files, renames and deletes; a document's history beyond its last change.

## Decisions

- **Two libraries**, added to `apps/web` alone with exact versions and the lockfile in the same commit (`pnpm --filter @catervas/web add --save-exact markdown-it@15.0.2 mermaid@11.17.2`), each recorded in the pull request as `docs/standards/code.md` asks: `markdown-it` 15.0.2 (MIT, released 2026-09-11, ships its own types), and `mermaid` 11.17.2 (MIT, 2026-08-25, the last of 11; 12.0.0 is a new major and 12.1.0 is under two weeks old). Neither is in the repository today. Rejected: `react-markdown` (the unified stack, a dozen packages); `marked`, which passes raw HTML through and would need a sanitizer.
- **Why `html: false` is enough.** `markdown-it` is made with `{ html: false, linkify: false, typographer: false }`. With `html: false` it makes no raw-HTML token: every `<`, `>`, `&` and `"` of the source comes out escaped. Its `validateLink` drops a link or image whose URL is `javascript:`, `vbscript:`, `file:`, or `data:` other than `data:image/gif|png|jpeg|webp`, whatever the letter case, and leaves the Markdown as text. So its output is set with `dangerouslySetInnerHTML`, in `Markdown.tsx` alone, under `// biome-ignore lint/security/noDangerouslySetInnerHtml: markdown-it with html: false escapes all HTML and drops unsafe links`. Agent text elsewhere stays React elements (`apps/web/src/components/MessageText.tsx:14`). The served policy (`crates/runtime/src/daemon/app.rs:70`: no inline script, images from `'self'` and `data:` only) is the second wall. Rejected: DOMPurify on top, a third library for what `html: false` already guarantees.
- **Mermaid, lazily, as an image.** A fence whose info is `mermaid` renders (a fence rule of `Markdown.tsx`) as `<figure>` holding its escaped source in a `<pre>`. After a render that holds one, `import("mermaid")` (a chunk of its own, loaded only then) is initialized once with `{ startOnLoad: false, securityLevel: "strict", theme: "neutral", fontFamily: "system-ui, sans-serif", htmlLabels: false }` (the root option, which mermaid 11 reads for every diagram); each figure's source goes through `render(<unique id>, source, figure)`, and the figure then holds `<img alt="Diagram" src="data:image/svg+xml;charset=utf-8,<encodeURIComponent(svg)>">`. An image because the app's `style-src 'self'` drops the `<style>` mermaid writes into its SVG, and an SVG image applies its own styles, runs no script and loads nothing, so the policy stays as it is. Labels are SVG text (`htmlLabels: false`), since HTML in `foreignObject` is not reliable inside an image; the figure's CSS sets `system-ui, sans-serif` too, so labels are measured in the font they are drawn in. `Markdown.module.css` gives the diagram image a light surface in both themes, the light theme's `surface` (`packages/brand/tokens/tokens.json:5`), since mermaid's `neutral` theme draws for a light page, and padding `var(--catervas-space-2)`. A source mermaid refuses stays in its `<pre>`, under "This diagram could not be drawn.". The e2e run under the served policy is the proof; if mermaid needs more of it there, that is a question back to the planner, not a change to the policy. Rejected: `'unsafe-inline'` styles for the whole app; `securityLevel: "sandbox"`, whose frame the policy refuses.
- **Routes**: `/files` and `/files/*` in `App.tsx` (`:62-87`), the splat being the path under `docs/catervas/` (`/files/product/roadmap.md`). `/files` shows the first document of the first folder that has one; a path the tree does not list shows "There is no such document.".
- **Where it is.** "Files" joins `PLACES` after "Chats" (`apps/web/src/shell/Shell.tsx:20-27`), as the mockup's rail has it. The phone bar keeps its five places (`:46` leaves out `/files` too), and on a phone `/team` shows a "Files" link before the "Settings" one (`:91-93`); the Files page on a phone starts with a "Back" link to `/team` (`PhoneFiles.dc.html`).
- **The tree (1024 px and wider, `useWide`, `:30`).** A `<nav>` labelled by the heading "Files", the lead "What your team knows about this project, one folder for each role. Each folder is written by its owner.", then one `<details>` per folder, the open document's folder open: its summary holds the owner's `Avatar` (32), a `RoleTag` in the folder role's colour reading the folder's name ("Product"), and the owner's name; inside, each document's name as a link, the open one `aria-current="page"`. A folder with no document has no `<details>`, and reads "Nothing here yet". The owner is `files.tree`'s `agent_id`, looked up in `team.get`'s agents as `Chats.tsx:22-25` does. `RoleTag` (`packages/ui/src/RoleTag.tsx:16`) gains `label?: string`, shown in place of the role's short name, its `title` still the role's name.
- **The phone picker** (narrower): a `<select>` labelled "File", one `<optgroup label="<Folder>, <owner>">` per folder, each option "<Folder> › <document>", an empty folder one disabled option "<Folder>: nothing here yet"; under it the owner's avatar and "<owner>’s folder, <Folder>". Choosing an option goes to its route.
- **The document.** A crumb "<Folder> › <document>", the name as a heading, the changed line, and "Edit" (not on the team's version, nor when `editable` is false: a marketing plan changes through a new version on its own page, step 04). A pair has a group labelled "Which version" of two `aria-pressed` buttons, "For you" (pressed when it opens) and "For the team" (the `.agent.md`). Under the head: a one-file document says "Written for your team. You can read it and change it."; the team's version says "This is how your team reads the <name, its first letter lower case>. To change it, change the version for you; <owner> keeps this one in step.", and, when `stale`, before that line: with `rederiving`, the owner's avatar and "<owner> is bringing this up to date with your change."; without (a twin made stale by the owner's own git commit, which gets no re-derive, step 04), "The team's version is older than yours", naming no agent. Then the version's text through `Markdown`.
- **The changed line**: "Changed by <who> <when>", then ", in " and a link to `/tasks/<task_id>` (`App.tsx:72`) when the version's `last_change` names a task. `<who>` is the agent's name when `agent_id` is on the team, "you" when `by_you`, else `author`. `<when>` is `timeAgo(at, now)`: "just now" under a minute, else `Intl.RelativeTimeFormat("en", { numeric: "auto" })` in the largest whole unit of minutes (under 60), hours (under 24), days (under 30), months (under 12) or years ("5 days ago", "yesterday"). No line when `last_change` is null. Nothing in the app says "N days ago" today (`pastDay`, `apps/web/src/pages/orders.ts:150`, says days).
- **Editing.** "Edit" replaces the text with the editor: for a pair "You are changing the version for you."; a group of two `aria-pressed` buttons "Write" (pressed) and "Preview"; under Write the hint "A line starting with # is a heading, one starting with - is a list item, and **two stars** make words bold." and a textarea labelled "<name> text" holding the version's text; under Preview the draft through `Markdown`; then "Save" (primary) and "Cancel" (quiet), and under them what saving does by `files.tree`'s `integration` (an owner's save is a folder change, integrated as an accepted task's branch is; the controller's ruling of 2026-10-10): under `auto_merge` the mockup's "Saving keeps your change at once." with, for a pair, " <owner> will update the team’s version."; under `pull_request` "Saving puts your change in a pull request; it is in your project once that is merged." and under `manual` "Saving puts your change on a branch for you to add with catervas integrate.", each with, for a pair, " Then <owner> updates the team’s version." The draft and its `base` (the version's `blob` when Edit was pressed) are the editor's own state, which the page's re-queries after events (`app/store.ts:29`) never touch.
- **"Open in Files" on Today's card** (step 03b's `DocumentChanges`, `apps/web/src/pages/DocumentChanges.tsx`, which defers it here): under each document's name, a link "Open in Files" to `/files/<path within docs/catervas/>` (`/files/product/roadmap.md`), the page's own route. Rejected: `/files?path=<path>`, a second address for one page.
- **Saving.** `client.call("files.save", { path, text, base })`. Answered: the editor closes and `files.tree` and `files.read` are asked again (`again()`).
- **A saved change still waiting** (`files.read`'s `waiting`, the text then being what the owner saved): in place of "Edit", a line by policy: `auto_merge` "Saved. Catervas adds it to your project in a moment.", `pull_request` "Saved. It goes out with a pull request.", `manual` "Saved. It goes out when you run catervas integrate folder-<change>." It goes when `waiting` is null again, which the page's re-query after `folder_change.integrated` brings. Nothing else in the page says "at once". Refused with `file_changed` (`refusalsOf`, `app/refusals.ts:140`): above the editor, as an alert, "This file changed while you were editing. Your text is still here; copy it, then reload to see the new version." and "Reload", the draft kept; "Reload" asks `files.read` again and puts its text and `blob` into the editor, and the alert goes. Any other refusal: `saidAll` (`:151`) in the same place, the draft kept; `refusals.ts`'s `WORDS` (`:9`) gains `file_busy` "This file has changes on your computer that are not in the project yet. Commit them or put them aside, then save again.", `marketing_plan` "A marketing plan changes through a new version on its page.", `change_waiting` "An earlier change to this file is not in your project yet. Edit it once it is." and `is_a_link` "This file is a link, which Catervas does not write through." "Cancel" closes the editor and sends nothing.

## File map

```
apps/web/package.json, pnpm-lock.yaml                      modifies: the two libraries (Task 1)
apps/web/src/components/Markdown.tsx, Markdown.module.css   creates (Task 1)
apps/web/src/components/Markdown.test.tsx                   tests (Task 1)
apps/web/src/pages/files.ts, files.test.ts                  creates: the wire types, timeAgo (Task 2)
apps/web/src/pages/Files.tsx, Files.module.css              creates: tree, picker, routes' page (Task 2)
apps/web/src/pages/FileDocument.tsx                         creates: the document (Task 2); the editor (Task 3)
apps/web/src/pages/Files.test.tsx                           tests (Tasks 2, 3)
apps/web/src/app/App.tsx, apps/web/src/shell/Shell.tsx, Shell.test.tsx   modifies (Task 2)
packages/ui/src/RoleTag.tsx                                 modifies: label (Task 2)
apps/web/src/strings/en.ts                                  modifies: the words (Tasks 1, 2, 3)
apps/web/src/app/refusals.ts                                modifies: file_busy, marketing_plan, change_waiting, is_a_link (Task 3)
apps/web/src/pages/DocumentChanges.tsx, DocumentChanges.test.tsx   modifies: Open in Files (Task 3)
apps/web/e2e/files.spec.ts, apps/web/e2e/fixtures/serve.ts  creates; modifies: commitFiles (Task 4)
docs/SPEC.md, docs/design/catervas-folders.md               modifies (Task 5)
```

## Interfaces

Consumes: step 04's `files.tree`, `files.read`, `files.save` (its `filesTreeResult` with `integration`, `filesReadResult` with `waiting`, `filesSaveResult` `{ change, branch }`, refusal codes `file_changed`, `is_a_link`, `change_waiting` and the four of `check_owner_edit`) and `"files.save"` in `MethodName` (`packages/protocol-client/src/client.ts:30-61`); `useQuery` (`app/store.ts:29`), `useConnection` (`app/connection.tsx:188`), `useWide` (`Shell.tsx:30`), `t` (`strings/t.ts`), `refusalsOf`, `saidAll` (`app/refusals.ts:140`, `:151`), `Avatar`, `RoleTag`, `Button`, `TextArea` (`@catervas/ui`), `Agent`, `Team` (`pages/setup/TeamSetup.tsx`), `renderApp`, `answerQuery`, `eventArrives` (`test/render-app.tsx:10`, `:55`, `:44`), `FakeSocket.calls`, `.reply`, `.fail` (`test/fake-socket.ts:69`, `:45`, `:49`), `expectNoAxeViolations` (`@catervas/ui/test`), `startServe`, `events`, `screenshots` (`e2e/fixtures/serve.ts:120`, `e2e/fixtures/shots.ts`); all on main or step 04.

Produces:

```ts
// apps/web/src/components/Markdown.tsx
export function Markdown({ text }: { text: string }): JSX.Element;
// apps/web/src/pages/files.ts (camelCase, as the client maps the wire)
export type Integration = "auto_merge" | "pull_request" | "manual";
export type FilesTree = { integration: Integration; folders: { folder: string; name: string; role: Role; agentId: string; documents: { path: string; name: string; twin: string | null }[] }[] };
export type FileVersion = { path: string; text: string; blob: string; lastChange: { at: string; taskId: string | null; agentId: string | null; author: string; byYou: boolean } | null };
export type FileRead = { document: FileVersion; twin: FileVersion | null; stale: boolean; rederiving: boolean; editable: boolean; waiting: { change: number; branch: string } | null };
export function timeAgo(at: string, now: Date): string;
// apps/web/src/pages/Files.tsx, FileDocument.tsx
export function Files(): JSX.Element;
export function FileDocument(props: { tree: FilesTree; path: string; agents: Agent[] }): JSX.Element;
// packages/ui: RoleTag({ role, label }: { role: Role; label?: string })
// apps/web/e2e/fixtures/serve.ts
export function commitFiles(project: string, files: Record<string, string>, message: string): void;
```

## Tasks

Component tests are Vitest with jsdom and Testing Library (`apps/web/vitest.config.ts`), run by `pnpm check`; the journey is Playwright (`apps/web/e2e/playwright.config.ts`), run by `cargo xtask check --integration` (`xtask/src/check.rs:140`).

### Task 1: Markdown, with its diagrams

Files: the libraries; `Markdown.tsx`, `Markdown.module.css` (the figure's font, the image's `max-width: 100%`, its light surface and padding), `Markdown.test.tsx` (`vi.mock("mermaid", …)` with `initialize` and `render` spies); `en.ts` ("Diagram", "This diagram could not be drawn.").

- `renders headings lists emphasis and code` — `# A\n\n- **b**\n\n`c`` gives a heading "A", a list item whose `strong` reads "b", and a `code` "c". RED: no component.
- `shows raw html as text` — `<script>alert(1)</script>` and `<img src=x onerror=alert(1)>` make no `script` or `img` element, and both texts are on the page; a `mermaid` fence holding `<img src=x onerror=alert(1)>` makes no `img` before `render` resolves (its spy held unresolved). RED.
- `leaves unsafe links as text` — `[a](javascript:alert(1))`, `[b](JavaScript:alert(1))`, `[c](vbscript:x)` and `[d](data:text/html,x)` make no `a` element; `[e](https://example.com/)` makes one with that `href`. RED.
- `loads mermaid only for a document with a diagram` — a document with no `mermaid` fence never calls `initialize`; one with a fence `flowchart LR\n  A --> B` calls `initialize` once with `securityLevel: "strict"`, `startOnLoad: false` and the root `htmlLabels: false`, and `render` with that source, and its figure then holds an `img` named "Diagram" whose `src` starts `data:image/svg+xml`. RED.
- `keeps a diagram mermaid refuses as its source` — `render` rejecting leaves "This diagram could not be drawn." and the source in a `pre`, and no `img`. RED.
- `has no accessibility violations` — `expectNoAxeViolations` on a rendered document with a heading, a list and a drawn diagram.

- [ ] `feat(web): render Markdown with its Mermaid diagrams`

### Task 2: Browse and read

Files: `files.ts`, `files.test.ts`, `Files.tsx`, `Files.module.css`, `FileDocument.tsx` (head, switch, notices, text), `Files.test.tsx` (`renderApp("/files/…")`, `answerQuery` of `team.get` with Mira the Product Manager, Ada the Architect, Iris the UI/UX Designer, then `files.tree` and `files.read`; the clock fixed with `vi.setSystemTime` as `orders.test.tsx:19` does), `App.tsx`, `Shell.tsx`, `Shell.test.tsx` (`orders_the_rail`, `:49`), `RoleTag.tsx`, `en.ts`.

- `says how long ago in words` (`files.test.ts`) — against 2026-10-10T12:00:00Z: 30 s before is "just now", 5 min "5 minutes ago", 3 h "3 hours ago", 1 day "yesterday", 5 days "5 days ago", 45 days "last month", 400 days "last year". RED: no function.
- `orders_the_rail` (updated) — the rail is Today, Board, Chats, Files, Team, Costs, Settings; the phone bar Today, Board, Chats, Team, Costs; on a phone `/team` shows links "Files" and "Settings" in that order. RED: no Files.
- `lists each folder with its owner` — Product with Mira's avatar, a tag reading "Product" titled "Product Manager", and "Mira"; Architecture's documents "Overview", "Plan: pie pre-orders" as links to `/files/architecture/overview.md` and `/files/architecture/plans/CTV-12.md`; Design reads "Nothing here yet" and has no `details`. RED: no page.
- `opens a pair on the version for you` — at `/files/product/roadmap.md`: crumb "Product › Roadmap", heading "Roadmap", "For you" pressed, the human text, "Edit" present; pressing "For the team" shows the twin's text, "This is how your team reads the roadmap. To change it, change the version for you; Mira keeps this one in step.", and no "Edit". RED.
- `says who changed it when and in which task` — `lastChange` 5 days old with `taskId` CTV-9 and `agentId` mira reads "Changed by Mira 5 days ago, in CTV-9", CTV-9 a link to `/tasks/CTV-9`; one with `byYou` and no task "Changed by you just now"; one by author "Sam" with no agent "Changed by Sam …" with no task link; `null` shows no line. RED.
- `says when the team's version is being brought up to date` — `stale: true` and `rederiving: true`, "For the team": "Mira is bringing this up to date with your change." beside Mira's avatar; "For you" does not show it; with `rederiving: false`, "The team's version is older than yours" and no agent named. RED.
- `says a one-file document is written for the team` — overview: no "Which version" group, "Written for your team. You can read it and change it.", "Edit" present; a marketing plan (`editable: false`) has no "Edit". RED.
- `opens the first document at files` — `/files` shows "Product description", the tree's first. RED.
- `picks a document on a phone` — narrow: "Back" links to `/team`; the select "File" has optgroups "Product, Mira", "Architecture, Ada", "Design, Iris", the option "Design: nothing here yet" disabled, "Mira’s folder, Product" shown; choosing "Architecture › Overview" goes to `/files/architecture/overview.md`. RED.
- `has no accessibility violations` — `expectNoAxeViolations` wide and narrow.

- [ ] `feat(web): browse the team's folders and read their documents`

### Task 3: Edit and save

Files: `FileDocument.tsx` (the editor, the waiting line), `Files.test.tsx`, `en.ts`, `refusals.ts`, `DocumentChanges.tsx`, `DocumentChanges.test.tsx`.

- `edits and saves the version for you` — "Edit" on roadmap: "You are changing the version for you.", "Write" pressed, the hint, the textarea "Roadmap text" holding the text; typing then "Preview" renders the draft's heading; "Save" sends one `files.save` with `{ path: "docs/catervas/product/roadmap.md", text: <draft>, base: <document blob> }`, and its answer `{ change: 1, branch: "docs/folder-1" }` closes the editor and asks `files.read` again; under `auto_merge` "Saving keeps your change at once. Mira will update the team’s version." is shown while editing. RED: no editor.
- `says what saving does under each policy` — under `pull_request` the editor reads "Saving puts your change in a pull request; it is in your project once that is merged. Then Mira updates the team’s version.", under `manual` "Saving puts your change on a branch for you to add with catervas integrate. Then Mira updates the team’s version."; neither holds "at once". RED.
- `says a saved change is on its way` — `waiting: { change: 3, branch: "docs/folder-3" }`: no "Edit"; under `auto_merge` "Saved. Catervas adds it to your project in a moment.", `pull_request` "Saved. It goes out with a pull request.", `manual` "Saved. It goes out when you run catervas integrate folder-3."; a new `files.read` with `waiting: null` brings "Edit" back. RED.
- `keeps the draft while events arrive` — an event (`eventArrives`) and a new `files.read` answer while editing leave the textarea's text and the `base` sent unchanged. RED.
- `keeps the owner's text when the file changed` — `fail` with `-32005` and `{ errors: [{ path: "/base", code: "file_changed", message: "…" }] }`: an alert reads "This file changed while you were editing. Your text is still here; copy it, then reload to see the new version.", the textarea still holds the draft; "Reload" asks `files.read`, whose new text and blob fill the textarea, the alert gone, and the next "Save" sends the new blob. RED.
- `says any other refusal and keeps the draft` — `fail` with code `change_waiting` (`/path`) shows "An earlier change to this file is not in your project yet. Edit it once it is.", with `file_busy` "This file has changes on your computer that are not in the project yet. Commit them or put them aside, then save again.", and with `too_large` (`/text`) `saidAll`'s words; the draft stays. RED.
- `opens a changed document in files` (`DocumentChanges.test.tsx`) — the card's "Roadmap" has a link "Open in Files" to `/files/product/roadmap.md`, and "Product description" one to `/files/product/spec.md`. RED: no link.
- `cancels without saving` — "Cancel" closes the editor, the version's text shows, and no `files.save` was sent. RED.
- `offers no edit of the team's version` — on "For the team" there is no "Edit"; a one-file document's editor has no "You are changing the version for you." and no owner line. RED.

- [ ] `feat(web): edit a document and save it as the owner's change`

### Task 4: The journey

Files: `e2e/fixtures/serve.ts` (`commitFiles`: writes each file, `git add` and `git -c user.name=t -c user.email=t@example.com commit -m <message>` in the project, as `gitProject` does at `:88`); `e2e/files.spec.ts`, its screenshots `files`, `files-edit`, `files-changed`, `files-stale` and `files-phone` taken as `chats.spec.ts` takes its own.

- `the owner reads and edits the team's documents, through the real server and browser` — `startServe({ team: "pm-architect-developer", transcripts: [] })`, then `commitFiles` of `docs/catervas/product/roadmap.md` (`# Roadmap\n\nPie pre-orders.\n`), its twin (`# Roadmap\n\nNow: pie pre-orders.\n`) and `docs/catervas/architecture/overview.md` with a `mermaid` fence `flowchart LR\n  Shop --> Orders`: (1) the rail's "Files" shows Product (Mira), Architecture (Ada), Engineering (Theo) "Nothing here yet"; (2) Overview shows an image "Diagram" whose `naturalWidth` is over 0; (3) Roadmap's "For the team" shows "Now: pie pre-orders."; (4) "For you", "Edit", the footnote reads "Saving keeps your change at once. Mira will update the team’s version." (`catervas init`'s policy is `auto_merge`), a new line typed, "Save": the typed line is shown and "Changed by you just now" (once the tick has merged the change, within 15 s), and `events(project)` holds one `folder_doc.edited` with that path, one `folder_change.integrated` and one `task.created` whose `rederives` is `docs/catervas/product/roadmap.agent.md`; "For the team" then reads "Mira is bringing this up to date with your change."; (5) "Edit", another line typed, `commitFiles` changing `roadmap.md`, "Save": the alert, the typed text still in the box; "Reload" puts the committed text in it; (6) at 390 × 844, `/team`'s "Files", then "Architecture › Overview" in "File", reaches `/files/architecture/overview.md`. RED: no page.

- [ ] `test(web): read and edit a document in the browser`

### Task 5: Spec

`docs/SPEC.md` 4.5, "Reading and changing the team's documents" (added in the next spec revision): the Files page as the Decisions build it (where it is, the tree and the phone picker, the two versions, the changed line, the stale line, editing and the Preview, Save as the owner's change, what the page says it does under each integration policy, the waiting line, `file_changed`, `file_busy` and `change_waiting`, no edit of a marketing plan, "Open in Files" on Today's card), Markdown with raw HTML off and unsafe links left as text, diagrams drawn by Mermaid only on a document that has one, as an image, under `securityLevel: "strict"`. `docs/design/catervas-folders.md`, "The Files page": the stale twin's words become "<agent> is bringing this up to date with your change." (the approved mockup's), and the waiting-proposal sentence gains "(a later step)".

- [ ] `docs(spec): record the Files page`

## Verification

```
cargo xtask check --integration      # or /tmp/claude-0/fullcheck.sh in the cloud container; Task 4 is a Playwright journey
# expected: xtask check: ok
pnpm --filter @catervas/web e2e files.spec.ts
# expected: 1 passed
pnpm licenses list --prod --filter @catervas/web
# expected: every licence compatible with Apache 2.0; the list, transitive ones included, goes in the pull request
```

The pull request lists, for the founder, the words the page shows by integration policy that no mockup draws: the waiting lines ("Saved. Catervas adds it to your project in a moment.", "Saved. It goes out with a pull request.", "Saved. It goes out when you run catervas integrate folder-<n>.") and the `pull_request` and `manual` footnotes, with their screenshots.
