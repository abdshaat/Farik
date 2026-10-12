# 0058. The team writes its documents as HTML

Date: 2026-10-12
Status: accepted (the founder's decisions of 2026-10-12 in conversation, recorded in `docs/design/html-documents.md`, whose design the founder approved the same day: "approve. Also make sure the user can view the html fitted to thier screens."). Amends ADR 0051 (a document under `docs/catervas/` is `.md` or `.html`, a human document's agent version has its format, and the Files page shows HTML).

## Context

ADR 0051 made every team document Markdown: `catervas-core` fixes the `.md` and `.agent.md` names, the write tool and the owner's save refuse anything else, and the Files page renders Markdown with raw HTML off and Mermaid for diagrams. The founder wants documents written as HTML, which shows diagrams, flowcharts and analytics far better, while a team's existing Markdown documents stay Markdown until the owner asks for a rewrite. Agent-written HTML is untrusted text shown inside the app that holds the owner's session, so how it is shown is a security decision as much as a format one.

The options were:

- **Either format, the team's setting deciding new documents.** A document is a name plus `.md` or `.html`; every rule that names a fixed document matches the name, whichever format is there. Existing documents keep their format; a rewrite is a separate, owner-started job. It costs a format-neutral rewording of every role's prompt and skill, and a second viewer.
- **Markdown written, HTML shown.** Agents keep Markdown and the page renders it richer. It changes nothing for the agents, who would still draw with Markdown, which is what the founder wants changed.
- **HTML only, at once.** One extension swapped everywhere. Every existing team's documents stop matching their rules, which the founder ruled out.

For showing HTML: rendering it into the page (a sanitizer, or the page's own policy loosened for inline styles) puts agent-written markup in the app's origin; a `srcdoc` frame inherits the app's policy, whose `style-src 'self'` strips every document's styles. A route of its own answers each document under `Content-Security-Policy: sandbox`, framed with `sandbox` and no `allow-*`, so the document runs in an opaque origin with no script and no network.

## Decision

A team's documents are `.md` or `.html`, new ones in the team's `policy.document_format`, `html` unless set, and an existing document keeps its format until the owner's "Rewrite documents as HTML" files a task for each role to rewrite its folder. HTML documents hold no scripts and load nothing, and the browser reads them only through `GET /documents/raw`, a sandboxed frame under the route's own policy, with a fit block that keeps them to the reader's screen.

## Consequences

- Diagrams, flowcharts and charts are inline SVG the agents draw themselves, with no renderer to load; the Mermaid path stays for Markdown documents.
- Every rule that named a fixed `.md` document now matches by name, so a mixed folder (an old `spec.md`, a new `overview.html`) works without migration, and nothing is converted behind the owner's back.
- HTML costs more tokens than Markdown to write and to read, agent versions included; the founder chose HTML agent versions knowing it.
- A line diff of HTML is only readable when agents keep one block per line, which the prompt asks for and nothing enforces; the owner's view of a change strips tags, and the agents' line rule is what makes it short.
- Docs tasks are held to the format by their prompt alone; a Markdown document added to an HTML team lands and reads as any other.
- An HTML document has no live preview while the owner edits its source, and no script-driven interaction, ever, without a new decision on how documents are shown.
