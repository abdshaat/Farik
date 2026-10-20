# Attached documents: the owner gives the team a spec, a description or an architecture

Status: decided by the founder on 2026-10-19, in conversation (the four answers below), and by the planner (Opus 5.5) for the mechanism; ADR 0076; standalone plan `attached-documents`.

## The brief

The founder, verbatim, 2026-10-19: "Allow the user to include documents in the chat or in the main endpoint such as product spec, product description, or architecture."

Asked four questions, the founder answered:
1. Where an attached document lives: "Team's folders": committed under `docs/catervas/`, filed by kind, shown on the Files page, read by the whole team after the message.
2. Which files: "Text, PDF and Word": `.md`, `.txt`, `.pdf`, `.docx`, PDF and Word turned into text on the way in.
3. Where the attach button appears: "Today's request box, One-to-one chats, Team channel, Setup / new project".
4. Approval: "Becomes a team document": the folder's agent derives an `.agent.md` version, as for the product plan.

## What the owner sees

A paperclip button, "Attach a document", beside the send button of Today's request box, a one-to-one chat and the channel, and under the description on the setup's project screen. It opens the computer's file picker for `.md`, `.markdown`, `.txt`, `.pdf` and `.docx`. A picked file asks one question, "What is it about?": "The product (a spec or a description)" or "The architecture", the first pressed. The file is then sent, and a chip with its name shows above the box, with a spinner while it is sent and an "x" that takes it off the message (the document stays in the team's folder). A refusal shows under the box in Catervas's words. A message holds at most five documents.

Sent, the message, the request or the channel post shows each chip as a link to the document on the Files page. There the document is named by its first heading, else its file name; its line says "Attached by you from <file name>", and it has the "For you" and "For the team" switch of a pair, its "For the team" version written by the folder's agent.

On the setup's project screen the chips wait in the wizard and are sent when the owner presses the finish screen's start button, once the team exists.

## Where a document goes

| Kind | Folder | Owner |
|---|---|---|
| `product` | `docs/catervas/product/attached/` | Product Manager |
| `architecture` | `docs/catervas/architecture/attached/` | Architect |

The file is `<slug>.md`, its twin `<slug>.agent.md`. The slug is the file name without its extension, lowercased, each run of characters outside `a-z0-9` made one `-`, trimmed of `-`, cut to 60 characters, `document` when nothing is left; a slug the folder already holds, on the integration branch or in a folder change not yet integrated, takes `-2`, `-3`, and so on. A team with no active agent of the folder's role is refused `no_folder_owner` (a one-agent team whose agent is neither role among them).

## How it is turned into text

All in the daemon, at most 10 MiB per file:
- a file starting `%PDF-` is a PDF: its text, page by page, a blank line between pages; a PDF with no text (a scan) is refused `no_text`; an encrypted one `unreadable`.
- a zip holding `word/document.xml` is a Word file: each paragraph a line, a paragraph styled `Heading1` to `Heading6` (or `Title`, as 1) a `#` heading of that level, a numbered or bulleted one a `- ` item, a table each row one line of its cells joined by ` | `, runs' text joined, pictures dropped.
- anything else must be UTF-8 named `.md`, `.markdown` or `.txt`, kept as it is; other files are refused `unsupported_type`.

The text then meets the owner's edit limit, 256 KiB (`too_large`), and is committed as the owner's own change, as Save does (5.17). Its integration files the re-derive task, and the folder's agent writes the twin.

## How the agents read it

The message, the request or the post records the documents' paths. The session that answers it gets a section "Documents the owner attached", each document's text read from the integration branch, else from the branch of its folder change while that waits on its pull request, inside the untrusted-content notice, at most 64 KiB in all; past that it gives the path alone. After that the document is in the folder like any other. The Product Manager's interview of a new project lists the documents attached at setup by path.

## Out of scope

Pictures and diagrams, other file types (`.pptx`, `.odt`, `.rtf`), scanned PDFs (no OCR), attaching to an existing document's place (replacing the spec), other folders than product and architecture, and removing an attached document from the page (Files' own ways apply).
