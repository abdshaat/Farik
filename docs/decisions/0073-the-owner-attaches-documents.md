# 0073. The owner attaches documents, which become team documents in the role folders

Date: 2026-10-19
Status: accepted (the founder, 2026-10-19, quoted in `docs/design/attached-documents.md`; the mechanism decided by the planner)

Amends:
- **ADR 0051** (Catervas folders): a pair is no longer only the product plan and a marketing plan; an attached document is one too.
- **SPEC 4.2, 4.3, 4.5, 5.9, 5.17, 8.5** (revision 0.128).

## Context

The founder asked that the owner can include documents, "such as product spec, product description, or architecture", in the chats and in Today's request box, and chose: the team's folders, text, PDF and Word, the request box, the one-to-one chats, the channel and the setup, and a team document with an agent version.

Four facts constrain it: a chat message holds 4,000 characters and a channel post 2,000, so a document cannot ride in the text; nothing sends bytes from the browser to the daemon yet; neither the workspace nor the sandbox can read a PDF; and a folder change integrates by pull request (ADR 0061), so a document just attached may not be on the integration branch when the agent answers.

## Decision

1. **One upload method, `document.attach`.** `{ name, kind, data_base64 }`, `kind` `product` or `architecture`, at most 10 MiB decoded, answered `{ path, change }`. Rejected: a binary WebSocket frame or an HTTP upload route (a second path into the daemon beside the schema-checked JSON-RPC one, for files of a few megabytes).
2. **Turned into Markdown in the daemon.** PDF with the `pdf-extract` crate (pure Rust, MIT); Word with `zip` and `quick-xml`, already in the lock file through `calamine`. Rejected: `pdftotext` in the sandbox image (the daemon, not an agent, files the document, and the image would grow by poppler); sending the original (the agents read text, and Files shows Markdown and HTML alone).
3. **Filed by kind under `attached/`.** `docs/catervas/product/attached/<slug>.md` or `docs/catervas/architecture/attached/<slug>.md`. Rejected: one owner's folder (no agent owns it, so none writes a twin); asking for the folder (two kinds cover the founder's three examples).
4. **A pair, re-derived as an owner's edit.** `is_human_document` widens to the two `attached/` folders; the attach is the owner's own change, recorded `folder_doc.edited` as Save is, so its integration files the re-derive task by the existing routine, plus `document.attached` for the original's name and type.
5. **The message names the documents; the prompt carries them.** `request.file`, `chat_message_post` and `message_post` take `attachments`, at most five paths, recorded on `task.created`, `chat_message.posted` and `message.posted`; the session answering gets the documents' text, read from the integration branch or else the folder change's branch, untrusted, 64 KiB in all. Rejected: pasting the text into the message (the limits); a path alone (the document may not be integrated yet).
6. **Setup holds them until the team exists.** The wizard keeps the picked files and sends them at its finish; the interview lists them.

## Consequences

- Positive: a spec, a description or an architecture the owner already has reaches the team once, in their own words, and stays as the team's document.
- Negative: two new dependencies (`pdf-extract`, and `zip` and `quick-xml` made direct); a scanned PDF has no text and is refused; a document's layout (pictures, columns) is lost.
- Neutral: the folders, Save, the re-derive and the Files page work as before for every other document.
