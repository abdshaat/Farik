# HTML documents: the team writes its documents as HTML

Status: the design approved by the founder on 2026-10-12, in conversation ("approve. Also make sure the user can view the html fitted to thier screens."), with its three choices: agent versions in HTML too, an HTML document changed by asking the team or, under Advanced, by editing its source, and a "Rewrite documents as HTML" button in Settings (ADR 0058). Amends `docs/design/catervas-folders.md`.

## Why

The founder wants the team's documents written as HTML, not Markdown: HTML shows diagrams, flowcharts and analytics far better, and the founder holds it to suit agents better too. A team's existing Markdown documents stay Markdown until the owner asks the team to rewrite them; HTML is the default for every team.

## The founder's decisions (2026-10-12, in conversation)

- Agents write documentation as `.html` by default; a document already in `.md` stays `.md`.
- The owner may ask the team to rewrite all its documentation as HTML, and the team does it.
- A human document's agent version is HTML too: `spec.html` beside `spec.agent.html`.
- An HTML document is changed by asking its team ("Ask for a change"), or under Advanced by editing its source.
- The rewrite is a button in Settings.
- An HTML document is shown fitted to the reader's screen, a phone's included.

## What a document is

- A document under `docs/catervas/` is `<name>.md` or `<name>.html`. Its format is its extension; a human document's agent version has the same one (`<name>.agent.md`, `<name>.agent.html`).
- The fixed documents (the spec, the roadmap, the Catervafication documents, the marketing plans, `DOCUMENT_NAMES`) are known by name without the extension; whichever format is there counts. The product plan is approved when a spec and a roadmap, in either format, are on the integration branch.
- Only `.md` and `.html` files are documents; the Files page lists nothing else, as it lists only `.md` today.

## The setting

- `policy.document_format`, `html` or `markdown`, in `.catervas/team.yaml`. A team file without it is `html`: every team, an existing one included, writes its new documents as HTML. Templates carry it.
- Settings → Work gains a group "Documents" with one switch, "Write documents as HTML" (on: `html`; off: `markdown`), saved through `team.save` as "Plan work in sprints" is.

## Writing a document

- **New documents** are written in the team's format. **An existing document keeps its format** when it is changed: the agent changes `spec.md` as Markdown in an HTML team.
- `catervas_write_folder_doc` holds this:
  - a path in the team's format is written; when the same name exists in the other format (`cadence.md` for `cadence.html`), the same folder change removes that file, which is how an agent-only document is rewritten in place. The spec and the roadmap are not: writing `spec.html` while `spec.md` is there is refused with `other_format_exists`, since their rewrite reaches the owner through a task;
  - a path in the other format is written only when that file is already on the integration branch; otherwise it is refused with `wrong_format`, its message naming the path in the team's format.
- Docs tasks are held by their prompt, not by a new Definition of Done rule: a task that adds a Markdown document in an HTML team still lands, and its document is read as any other. (`ponytail:` no Definition of Done rule for the format; add one if teams drift.)
- The Team rules section of every session says, in an HTML team:

  > Documents: write a new document under docs/catervas/ as HTML (`.html`, a human document's agent version `.agent.html`): one self-contained page with `<meta name="viewport" content="width=device-width, initial-scale=1">`, one `<h1>`, styles in one `<style>` block, widths that fit a phone (percentages, `max-width`), diagrams, flowcharts and charts as inline SVG with a `viewBox`, one block element per line, no scripts and nothing loaded from anywhere else. A document already in `.md` stays Markdown when you change it. The kit names documents without their extension.

  In a Markdown team: "write a new document under docs/catervas/ as Markdown (`.md`, a human document's agent version `.agent.md`), diagrams in Mermaid blocks. A document already in `.html` stays HTML when you change it. The kit names documents without their extension."
- Every role's system prompt and skill names its documents without an extension (`docs/catervas/product/spec`, `architecture/overview`), and the Architect's "a Mermaid diagram" becomes "a diagram". The prompts Catervas writes itself (the sprint review's refresh, the retro's cadence line, the product plan's contract) name the file that is there, or the team's format for a new one.
- **The Architect's plans** are `architecture/plans/<epic>.<ext>` in the team's format; readiness accepts a contract's `plan` in either format.
- **A marketing plan** proposed in an HTML team is `MP-<n>.html` and `MP-<n>.agent.html`. The agent's `text` and `agent_text` are the page's body; the tool wraps each in a whole page with the viewport line, a `<title>` and `<h1>MP-<n>: <title></h1>`, as it puts `# MP-<n>: <title>` above a Markdown plan today. Plans already proposed keep their format.
- **Catervafication** in an HTML team files its documents as `.html`. Each artifact criterion asks for `<h1` (`# ` in Markdown), and the architecture overview also asks for `<svg` (a Mermaid block in Markdown).
- **The single-block-per-line rule** keeps the line diff of an HTML change readable. Today's "changes to approve" shows each changed line of an HTML document with its tags removed, so the owner reads words, not markup.

## Reading an HTML document

- **The document route.** `GET /documents/raw?path=<path>[&task=<id>]` answers an HTML document as the Files page reads it (the owner's waiting change included), or as the task's branch holds it when `task` names one (the product plan card reads its texts there). It needs the session cookie, as `GET /session` does: `403` from another host, `401` without a session, `404` for a path that is not an `.html` file under `docs/catervas/` the branch holds. Markdown documents are drawn in the page as today and never served here.
  - An HTML document is answered as `text/html; charset=utf-8` under its own policy: `Content-Security-Policy: sandbox; default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:; frame-ancestors 'self'`, with `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer` and `Cache-Control: no-store`. The document runs no script, reaches no network, cannot read the app's session or storage, and cannot be framed by another site.
  - **Fitted to the screen.** The route adds a fit block to every HTML document before its own styles: the viewport line, `body { margin: 0 auto; padding: 16px; max-width: 72rem; overflow-wrap: anywhere }`, `img, svg, video, canvas { max-width: 100%; height: auto }`, and `table, pre { display: block; max-width: 100%; overflow-x: auto }`. It is inserted after the opening `<head>` tag, else after the opening `<html>` tag, else after a doctype, else at the start. Because it comes first, a document's own styles win where they disagree, and nothing a document writes can widen the page past the screen except its own CSS.
- **The Files page** shows an HTML document in `<iframe sandbox src="/documents/raw?path=…">`: no `allow-*` token at all, titled with the document's name. The frame takes the full width of the reading column and the height of the window below the page's header (at least 60% of the window), and scrolls inside itself. On a phone the frame is the full width under the picker. Above it, "Open full page" opens the same route in a new tab (`rel="noopener"`), where the route's own policy still holds. Markdown documents render exactly as today.
- "For you / For the team" switches between the two files of an HTML pair as it does for Markdown.
- **The product plan card** on Today shows an HTML spec and roadmap the same way, from the task's branch.
- The app's own policy is unchanged: a frame from `'self'` is already allowed by `default-src 'self'`.

## Changing an HTML document

- **Ask for a change** (every document the owner may edit, either format). A box under the document, "What should change?", with "Send to <role name>". Sending (`files.ask_change { path, words }`, 1 to 2000 characters) files a small task for the role that owns the folder: "Change <document name>", the owner's words quoted as the owner's, `allowed_paths` the document and its agent version, triaged at once with no triage session, as the re-derive task is filed. A change to the spec or the roadmap still waits for the owner's acceptance, as any task that may change them does. Refused for a path the owner may not edit (`not_editable`), and when the folder's role has no active agent (`no_owner`).
- **Edit source** (Advanced, HTML documents only). The existing editor, on the document's HTML, with "Save" and "Cancel". There is no live preview, since an unsaved page has no route to be served from, so the frame shows the saved document once the save is in. Saving is the owner's edit as today (`files.save`): a folder change, and a re-derive of the agent version once integrated.
- A Markdown document keeps "Edit" with "Preview" as today.

## Rewriting the documents as HTML

- **The button.** Settings → Work → Documents shows "Rewrite documents as HTML" while the format is HTML and the Files page lists at least one `.md` document, with an ⓘ: "Each role rewrites its own documents as HTML. Marketing plans stay as they were approved."
- **The method** `files.rewrite` files, as the owner's word, one docs task for each role whose folder holds a `.md` document and that can take a task with a reviewer the team can staff, each triaged at once as a re-derive is, and records `document_rewrite.started { tasks }`. Each task names the folder's `.md` documents found on the integration branch when the button is pressed. With sprints on, the tasks wait in the Backlog for the next sprint, as any small task does.
  - A task: "Rewrite <role>'s documents as HTML", `allowed_paths` its folder, one artifact criterion per document (`<name>.html` exists and holds `<h1`; the overview also `<svg`), and a review rubric: every `.md` named is gone with its agent version, and each `.html` says everything its `.md` said, nothing more.
  - Marketing plans (`marketing/plans/MP-<n>.*`) are not rewritten: each is the version the owner approved.
  - The spec and the roadmap come back to the owner once, as any change to them does.
  - The Scrum Master never takes a task. While a rewrite is started and `delivery/` still holds a `.md` document, its next retro is told: "Rewrite the documents in docs/catervas/delivery/ as HTML with catervas_write_folder_doc; writing <name>.html removes <name>.md."
- **Refused** with `not_html` when the format is Markdown, `nothing_to_rewrite` when no folder of an active role holds a `.md` document other than marketing plans, and `rewrite_open` while a task of the last rewrite is neither accepted nor cancelled. The refusal's words are shown under the button.

## Events and wire

- `policy.document_format` (`team.schema.json`, the template schema, `team.updated`, `team.get`).
- Event `document_rewrite.started { tasks }`; methods `files.rewrite {}` and `files.ask_change { path, words }`, whose tasks carry `created_by: human`.
- `files.tree` lists `.html` documents; the page tells a document's format by its extension.
- `catervas_write_folder_doc` refusals `wrong_format` and `other_format_exists`; `files.save` accepts `.html` (its `not_markdown` becomes `not_a_document`).

## Out of scope

- Documentation outside `docs/catervas/`: README, CHANGELOG and code comments.
- Scripts in documents, so no interactive charts; a chart is drawn as SVG.
- A live preview of unsaved HTML, and a rich-text editor.
- Rewriting HTML back to Markdown: the switch changes new documents only.
- Rewriting approved marketing plans.
