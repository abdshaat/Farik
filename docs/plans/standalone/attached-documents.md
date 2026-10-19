# Standalone: the owner attaches documents

Status: ready
Branch: `feat/attach-documents` (its own pull request to `main`)
Design: `docs/design/attached-documents.md`; ADR 0073
Spec: `docs/SPEC.md` 4.2, 4.3, 4.5, 5.9, 5.17, 8.5, F7 (revision 0.128)
Depends on: `origin/main` at `ca21251`
Readiness confirmed by: the planner (Opus 5.5), 2026-10-19, against the three readiness rules (ADR 0065)

## Goal

The founder's brief (design, "The brief"). When done: the owner attaches `.md`, `.txt`, `.pdf` or `.docx` files, about the product or the architecture, in Today's request box, a one-to-one chat, the channel and the setup's project screen; each becomes a committed team document under `docs/catervas/<product|architecture>/attached/`, with an agent version its folder's agent writes; the session answering the message reads the documents. Out of scope: the design's "Out of scope".

## Decisions

- The upload, conversion, place, pairing, message and setup: ADR 0073 decisions 1 to 6, and the design's rules (slug, kinds, the three formats, the limits, the refusal codes `unsupported_type`, `no_text`, `unreadable`, `too_large`, `no_folder_owner`).
- Dependencies, exact pins in `[workspace.dependencies]`: `pdf-extract = "=0.12.1"`, `zip = { version = "=8.6.0", default-features = false, features = ["deflate"] }`, `quick-xml = "=0.41.0"` (the last two the versions `Cargo.lock` holds). Used by `catervas-runtime` alone.
- A PDF read that panics (pdf-extract can on malformed fonts) is caught with `std::panic::catch_unwind` and refused `unreadable`. Rejected: a child process (a second binary to ship).
- Conversion is in `crates/runtime/src/attach.rs`, not core: it parses bytes with third-party crates; core keeps the path rules, which are pure.
- `data_base64` is standard base64 with padding, decoded with the workspace's `base64`; at most 14,000,000 characters in the schema (10 MiB decoded is 13,981,016), the 10 MiB checked after decoding (`too_large`).
- `document.attach` is a method in `daemon/documents.rs` beside `files.save`, under the integration lock, committing with `tools::folders::land` and the message `docs(<product|architecture>): attach <slug>.md`, recorded `folder_doc.edited` then `document.attached`.
- New event `document.attached { path, name, media_type }`, `name` the original file name (1 to 200 characters), `media_type` one of `text/markdown`, `text/plain`, `application/pdf`, `application/vnd.openxmlformats-officedocument.wordprocessingml.document`; no agent, no session. Name follows rule 7.
- `attachments`: an optional array of 1 to 5 paths on `request.file` params, the `chat_message_post` and `message_post` command bodies, and the `task.created`, `chat_message.posted` and `message.posted` events. Each must be an attached path (core `is_attached_path`) that a `document.attached` event recorded, else refused `not_attached` at `/attachments/<i>`; more than five is refused `too_many_attachments` at `/attachments`. The texts' existing minimums and maximums are unchanged: a message with documents still needs its words.
- The prompt section is titled `## Documents the owner attached`, one `untrusted_block("document <path>", text, …)` per document in the order given, 64 KiB (`ATTACHED_BYTES = 64 * 1024`) shared in order; a document that does not fit gets the line "`<path>`: read it from the folder." Its text is read with `Git::file_at` from the integration branch, else from the branch of the folder change whose paths hold it, else the line alone.
- Which session gets the section: the triage, refine and implement sessions of a task whose `task.created` has attachments; the Chat session answering a chat message that has them (the newest human message's); the Conversation session started by a channel post that has them.
- Interview: after the owner's description, `They attached: <path>, <path>.` listing every `document.attached` path in log order, when there is one.
- `files.read` answers `attached: { name } | null` for an attached document, `name` from its `document.attached`; the page's line under the name reads "Attached by you from <name>".
- Web: one component file, `apps/web/src/components/Attach.tsx`, exporting `AttachButton` and `AttachedChips`, and the hook `useAttachments()` returning `{ items, add(file, kind), remove(path), paths, busy, refusal }`; the setup keeps picked `File`s in the module-level store `pendingSetupFiles` of the same file and sends them in `SetupFinish` before its existing start call. Strings in `apps/web/src/strings/en.ts`: `attachButton` "Attach a document", `attachKindQuestion` "What is it about?", `attachKindProduct` "The product (a spec or a description)", `attachKindArchitecture` "The architecture", `attachSend` "Attach", `attachRemove` "Take {name} off this message", `attachedFrom` "Attached by you from {name}", and one per refusal code: `attach_unsupported_type` "Catervas reads Markdown, text, PDF and Word files.", `attach_no_text` "This PDF has no text Catervas can read. It may be a scan.", `attach_unreadable` "Catervas could not open this file.", `attach_too_large` "This file is too large. Catervas takes up to 10 MB, and up to 256 KB of text.", `attach_no_folder_owner` "No one on the team keeps these documents. Add a Product Manager or an Architect.", `attach_too_many` "A message holds up to five documents."
- One test per major functionality (ADR 0060), named below.

## File map

```
Cargo.toml                                        modifies: the three pins
crates/runtime/Cargo.toml                         modifies: the three dependencies
crates/core/src/folders.rs                        modifies: AttachKind, attached paths, pairs; tests
crates/runtime/src/attach.rs                      creates: to_markdown; tests
crates/runtime/src/lib.rs                         modifies: mod attach
crates/runtime/tests/fixtures/attach/spec.pdf     creates: one page, the text "Product spec" and "It plans trips."
crates/runtime/tests/fixtures/attach/scan.pdf     creates: one page, no text
crates/runtime/src/daemon/documents.rs            modifies: document.attach; files.read's `attached`; tests
crates/runtime/src/daemon/web.rs                  modifies: the method allow-list
crates/runtime/src/daemon/board.rs                modifies: chat.messages and channel.messages answer attachments (Task 6)
docs/schemas/rpc.schema.json                      modifies: document.attach, request.file attachments, files.read answer
docs/schemas/command.schema.json                  modifies: attachments on two bodies
docs/schemas/event.schema.json                    modifies: document.attached; attachments on three events
crates/protocol/src/lib.rs, event.rs, command.rs  modifies: kind, mapping, reads of the new fields
packages/protocol-client/src/client.ts            modifies: MethodName gains document.attach
crates/runtime/src/daemon/gates.rs                modifies: request.file passes attachments
crates/store/src/requests.rs                      modifies: file_request takes attachments
crates/runtime/src/orchestrator/human.rs          modifies: chat and channel posts pass attachments; tests
crates/runtime/src/chat.rs                        modifies: NewChatMessage.attachments; interview; tests
crates/runtime/src/channel.rs                     modifies: NewMessage.attachments
crates/runtime/src/prompt.rs                      modifies: the section; tests
crates/runtime/src/orchestrator/session.rs        modifies: hands the paths to the prompt
crates/runtime/src/orchestrator/rules.rs          modifies: the conversation's paths
apps/web/src/components/Attach.tsx (+ .module.css, .test.tsx)   creates
apps/web/src/pages/Today.tsx, OneToOne.tsx, Channel.tsx         modifies: the button, the chips, attachments sent and shown
apps/web/src/pages/FileDocument.tsx               modifies: the attached line
apps/web/src/pages/files.ts                       modifies: FileRead.attached
apps/web/src/pages/setup/SetupProject.tsx, SetupFinish.tsx      modifies
apps/web/src/strings/en.ts                        modifies: the strings
docs/SPEC.md                                      modifies: revision 0.128
```

## Interfaces

Consumes: `is_human_document`, `agent_twin`, `check_owner_edit`, `OWNER_EDIT_LIMIT_BYTES`, `ROLE_FOLDERS` (`catervas-core`); `tools::folders::land`, `file_rederive`, `Git::file_at`, `folder_docs`, `untrusted_block`, `file_request`, `post_chat`, `channel::post` (on main).

Produces:

```rust
// catervas-core, folders.rs
pub enum AttachKind { Product, Architecture }
impl AttachKind { pub fn folder(self) -> &'static str; pub fn role(self) -> Role; pub fn parse(s: &str) -> Option<Self>; }
pub fn attached_slug(file_name: &str) -> String;
pub fn attached_path(kind: AttachKind, slug: &str) -> String;           // docs/catervas/<f>/attached/<slug>.md
pub fn free_attached_path(kind: AttachKind, file_name: &str, taken: impl Fn(&str) -> bool) -> String;
pub fn is_attached_path(path: &str) -> bool;
// catervas-runtime, attach.rs
pub const ATTACH_MAX_BYTES: usize = 10 * 1024 * 1024;
pub enum AttachRefusal { UnsupportedType, NoText, Unreadable, TooLarge }
impl AttachRefusal { pub fn code(self) -> &'static str; }
pub struct Converted { pub text: String, pub media_type: &'static str }
pub fn to_markdown(name: &str, bytes: &[u8]) -> Result<Converted, AttachRefusal>;
// catervas-store, requests.rs: file_request gains a last parameter `attachments: &[String]`
// catervas-runtime, chat.rs / channel.rs: NewChatMessage and NewMessage gain `pub attachments: Vec<String>`
// catervas-runtime, prompt.rs
pub const ATTACHED_BYTES: usize = 64 * 1024;
pub fn attached_section(documents: &[(String, Option<String>)]) -> Option<String>; // (path, text if read)
```

```ts
// apps/web/src/components/Attach.tsx
export type AttachKind = "product" | "architecture";
export type Attached = { path: string; name: string } | { pending: string; name: string };
export function useAttachments(): { items: Attached[]; paths: string[]; add(file: File, kind: AttachKind): Promise<void>; remove(key: string): void; busy: boolean; refusal?: string; clear(): void };
export function AttachButton(props: { onPick(file: File, kind: AttachKind): void; disabled?: boolean; full?: boolean }): JSX.Element;
export function AttachedChips(props: { items: Attached[]; onRemove?(key: string): void }): JSX.Element;
export const pendingSetupFiles: { file: File; kind: AttachKind }[];
```

## Tasks

### Task 1: attached paths are pairs

Files: modified `folders.rs`.

- `attached_documents_are_pairs_under_their_kinds_folder` — asserts `attached_slug("Product Spec v2 (final).PDF")` is `product-spec-v2-final`, of `"???.docx"` is `document`, of a 100-character name 60 characters with no trailing `-`; `free_attached_path(Product, "spec.md", |p| p.ends_with("attached/spec.md"))` is `docs/catervas/product/attached/spec-2.md`; `is_human_document` and `is_attached_path` are true for `docs/catervas/architecture/attached/api.md` and false for `…/attached/api.agent.md`, `…/attached/sub/api.md` and `docs/catervas/engineering/attached/api.md`; `agent_twin` of the first is `…/attached/api.agent.md`; `check_owner_edit` of it is `Ok`.

- [x] `feat(core): attached documents are pairs in the product and architecture folders`

### Task 2: turning a file into Markdown

Files: created `attach.rs`, the two fixtures; modified `Cargo.toml`s, `lib.rs`.

- `turns_markdown_text_pdf_and_word_into_markdown` — asserts `notes.md` bytes come back unchanged with `text/markdown`; `notes.txt` unchanged with `text/plain`; `spec.pdf` holds "Product spec" and "It plans trips." with `application/pdf`; a `.docx` built in the test with `zip` (a `Title` paragraph "Trips", a `Heading2` "Goals", a bulleted "Plan a trip", a 2×2 table `a|b`, `c|d`) is exactly `# Trips\n\n## Goals\n\n- Plan a trip\n\na | b\nc | d\n` with the Word media type.
- `refuses_what_it_cannot_read` — asserts `scan.pdf` is `NoText`; `photo.png` (PNG bytes) is `UnsupportedType`; `notes.md` holding invalid UTF-8 is `UnsupportedType`; a zip without `word/document.xml` named `.docx` is `UnsupportedType`; `%PDF-` followed by garbage is `Unreadable`; 10 MiB + 1 bytes is `TooLarge`; each `code()` is the design's snake_case word.

- [x] `feat(runtime): turn an attached PDF, Word or text file into Markdown`

### Task 3: `document.attach`

Files: modified `documents.rs`, `web.rs`, the three schemas' `document.attach` and `document.attached` parts, `lib.rs`/`event.rs` (protocol), `client.ts`.

- `attaches_a_file_as_the_owners_change_and_its_agent_writes_the_twin` — with a fixture team holding a Product Manager, asserts `document.attach { name: "Trip spec.md", kind: "product", data_base64 }` answers `{ path: "docs/catervas/product/attached/trip-spec.md", change: 1 }`; the folder change's branch holds the text at that path; the log has `folder_doc.edited` then `document.attached { name: "Trip spec.md", media_type: "text/markdown" }`; after the fixture integrates change 1, a `task.created` with `rederives: "docs/catervas/product/attached/trip-spec.agent.md"` exists; and `files.read` of the path answers `attached: { name: "Trip spec.md" }`.
- `refuses_an_attachment_with_a_code_at_its_field` — asserts `-32005` with `{ path: "/kind", code: "no_folder_owner" }` for `architecture` when the team has no Architect, `{ path: "/data_base64", code: "no_text" }` for `scan.pdf`, and that neither leaves a commit or an event.

- [x] `feat(runtime): the owner attaches a document to a role's folder`

### Task 4: messages and requests carry attachments

Files: the schemas' `attachments`; modified `gates.rs`, `requests.rs`, `human.rs`, `chat.rs`, `channel.rs`, `command.rs`.

- `a_request_a_chat_message_and_a_post_record_their_attachments` — after one `document.attach`, asserts `request.file { text, attachments: [p] }` records `task.created.attachments == [p]`, `chat_message_post` records `chat_message.posted.attachments == [p]`, `message_post` records `message.posted.attachments == [p]`, and a solo team's request (ADR 0071) carries them on its chat message.
- `refuses_attachments_never_attached` — asserts `docs/catervas/product/spec.md` and six attached paths are refused `not_attached` at `/attachments/0` and `too_many_attachments` at `/attachments`, with nothing recorded.

- [x] `feat(runtime): a request, a chat message and a channel post name the owner's documents`

### Task 5: the answering session reads them

Files: modified `prompt.rs`, `session.rs`, `rules.rs`, `chat.rs` (interview).

- `the_answering_session_reads_the_attached_documents` — asserts that a refine session of a task with one attachment, a Chat session after a message with one, and a Conversation session after a post with one each have the section with the document's text inside its untrusted block, read from the folder change's branch before integration and the integration branch after; that two documents of 40 KiB give the first's text and the second's "read it from the folder." line; and that the interview after a setup attach ends its first paragraph with `They attached: docs/catervas/product/attached/trip-spec.md.`

- [x] `feat(runtime): the session answering the owner reads the documents they attached`

### Task 6: the attach button in the request box, the chats and the channel

Files: created `Attach.tsx`, `.module.css`, `.test.tsx`; modified `Today.tsx`, `OneToOne.tsx`, `Channel.tsx`, `FileDocument.tsx`, `files.ts`, `en.ts`.

Amended in execution (2026-10-19): `chat.messages` and `channel.messages` (`crates/runtime/src/daemon/board.rs`, their results in `rpc.schema.json`) answer each message's `attachments` (omitted when none), each `{ path, name }` with `name` from its `document.attached`, so a sent message can show its chips; Task 4 recorded the paths but no query returned them.

- `attaches_a_document_and_sends_it_with_the_message` (Attach.test.tsx) — with a fake client, asserts that picking `spec.pdf` in Today's box and choosing "The architecture" calls `document.attach` with `kind: "architecture"` and the file's base64, shows a chip "spec.pdf", and "Send" calls `request.file` with `attachments: [path]`; that the chat's and the channel's boxes send `attachments` in their commands; that a refusal `no_text` shows `attach_no_text`'s words; that a sixth file shows `attach_too_many` without a call; and that a sent chat message shows its chip linking to `/files/<path>`.

- [ ] `feat(web): attach documents in the request box, the chats and the channel`

### Task 7: the setup

Files: modified `SetupProject.tsx`, `SetupFinish.tsx`, `setup.test.tsx`.

- `attaches_the_setups_documents_once_the_team_exists` (extended `setup.test.tsx`) — asserts the project screen's attach puts chips under the description without any call, and that the finish screen's start sends one `document.attach` per file, then its existing start call, and empties `pendingSetupFiles`.

- [ ] `feat(web): attach documents when starting a project`

### Task 8: spec

Records 0.128: 4.2 (the request box's attach), 4.3 (the chat's), 4.5 (the attached line on Files), 5.9 (the channel's), 5.17 (attached pairs, `document.attach`, its refusals, the conversion, the prompt section), 8.5 (`document.attached`, the `attachments` fields), F7.

- [ ] `docs(spec): record documents the owner attaches (0.128)`

## Verification

```
cargo xtask check
# expected: xtask check: ok
```

The founder's own test, in the web app: attach a Word spec in Today's box and a PDF architecture in the Architect's chat; the documents appear under Files, Product and Architecture, "Attached by you from …"; the answers quote them; after the folder changes integrate, "For the team" fills in.
