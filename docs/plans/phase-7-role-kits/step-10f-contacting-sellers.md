# Phase 7, step 10f: Contacting sellers

Status: draft. Its readiness review runs once step 10e has landed.
Branch: `phase/7-role-kits` (the phase branch; steps do not get their own)
Spec: `docs/SPEC.md` 5.7, 6.6, 6.7, 6.10, 8.4, 8.5, 8.6; F9
Depends on: steps 10b to 10e of this phase (the role, its folder, purchase orders, the kit, data pipelines); step 01 (the keychain store and `connectors.json`); step 02 and 10c (the human-only decision path and Today's rows); phase 6 (merged in #19)
Readiness confirmed by: not yet run

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

The founder's answer of 2026-10-05 (O1): the Procurement Specialist "compiles list of sellers, look for price, contact manufacturer or sellers. Get prices. And set up a purchase order but the final decision is the founder". After this step the agent can write to a seller or a manufacturer, asking for a quote, a price list, or an answer, from a procurement mailbox the user sets up, and read their replies. Every message is drafted by the agent and sent only when the founder presses "Send" on it, after reading it; the founder may edit it first. Replies are read from that mailbox only, only from addresses Farik wrote to, and reach the agent as untrusted content. An approved purchase order can be sent to its seller the same way. Out of scope: phone calls, contact forms on websites and chat widgets (the agent says to the founder where only those exist); the user's main mailbox; paying anyone.

## Decisions

- **A procurement mailbox of the user's own**, as the receipts mailbox of spec 6.6 is: a second address or alias at their own provider, such as `buying@` their domain, read over IMAP and sent from over SMTP with an app password or the provider's own password for mail programs, kept in the keychain (service `farik`, account `mailbox:<project_id>:procurement`) or, without one, in `connectors.json`, as step 01 keeps keys. Rejected: the user's main mailbox through Google's or Microsoft's mail API, for spec 6.6's reasons (a restricted scope with a paid yearly assessment, a grant over the whole mailbox); a connector of the kit, since sending must be Farik's own act after the founder's approval, not a tool the agent calls.
- **The first mailbox in Farik.** This step adds the IMAP and SMTP plumbing that phase 13 step 02's receipts intake reuses: `lettre` 0.11.23 (SMTP, rustls) and `async-imap` 0.12.0 (IMAP over the workspace's `tokio-rustls`), and `mail-parser` 0.11.9 to read a reply, each pinned exactly, licences recorded in the task report. TLS is required (implicit TLS on 993 and 465, or STARTTLS on 587); a server that offers neither is refused `mailbox_needs_tls`.
- **Connecting.** `procurement_mailbox_connect { address, imap_host, imap_port, smtp_host, smtp_port, username, signature, disclose_ai }` with the password through the keychain path, never the RPC echo: the web app's `ProcurementMailbox` page (mocked up first) and `farik procurement mailbox connect` (the password from standard input without echo, as `farik connect` reads keys). Connecting logs in to both servers and sends nothing; it records `mailbox.connected { purpose: procurement, address }`; `mailbox.disconnected` on removal, which deletes the password. Common providers' hosts are filled in from the address's domain (a fixed table: Gmail, Outlook and Microsoft 365, iCloud, Fastmail, Zoho, Proton through its Bridge on 127.0.0.1), each with its "how to make an app password" link.
- **What a draft is.** `farik_draft_seller_message { seller, to, subject, body, purpose, purchase_order? }`, `read` tier, Procurement Specialist only, in a session about a procurement task (`seller_message_refused` otherwise): `seller` 1 to 100 characters; `to` one address, parsed by `lettre`'s `Address`, no display name, no list (`seller_address_invalid`); `subject` 1 to 200 characters, one line; `body` plain text, 1 to 8,000 characters (`seller_message_too_long`); `purpose` `quote_request`, `question` or `purchase_order`; a `purchase_order` number only with that purpose, and only for an approved order (10c). It records `seller_message.drafted`, with the text in `.farik/local/procurement/mail/out/<n>.txt` and its sha256 in the event; at most 20 drafts wait at once (`seller_drafts_full`). No HTML, no attachments but Farik's own purchase-order workbook.
- **Only the founder sends** (under `ask`; step 10h adds `auto`, ADR 0041, which sends a draft at once within the cap). Today lists drafts under "Messages to sellers", each opening `SellerMessage` (mocked up first): from, to, the seller, subject, the whole body, the signature Farik will add, and "Send", "Edit", "Don't send". "Edit" lets the founder change subject and body; the sent text is the founder's then, `edited: true` on the event. "Send all" sends every draft on the screen, each its own event. The command `seller_message_send { message, subject?, body? }` is the human's alone (the daemon's token or the browser's cookie); `seller_message_discard { message }` records `seller_message.discarded`. Sending appends the signature and, when `disclose_ai` (on by default), the line "Written with an AI assistant and sent by <name> after reading it."; sets `Message-ID`, and records `seller_message.sent { message, message_id, sha256, edited }`, or `seller_message.failed { message, why }` in Farik's words when the server refuses, the draft kept to try again. At most 50 messages are sent a UTC day (`seller_send_limit`), so a mistake cannot become a mailing.
- **Replies.** Every 15 minutes while a process drives the project, and on "Check now", with no model, Farik reads the mailbox's INBOX by `BODY.PEEK` (nothing is marked read, moved or deleted) for messages newer than its ledger's last UID that answer a sent message (`In-Reply-To` or `References` holding a `Message-ID` Farik sent) or come from an address Farik sent to; anything else is never read past its headers. Each is kept under `mail/in/<yyyy-mm>/<n>/` (the text, and each PDF or image attachment up to 10 MB), the UID written to `mail/ledger.json`, and `seller_reply.received { reply, message, from, subject, attachments }` recorded. Today says "<seller> replied about <subject>" with "Ask for a comparison", which files an ordinary request ("Compare the replies to <subject>"), and "Dismiss" (`seller_reply.dismissed`).
- **What the agent reads.** `farik_read_seller_messages {}`: each draft and its state (`waiting`, `sent`, `failed`, `discarded`), with the text the founder actually sent; `farik_read_seller_replies { message? }`: each reply's sender, date, subject and text, inside the untrusted-content notice (spec 8.6), and the names of its attachments, which `Read` opens from the folder. A reply answers no question and approves nothing.
- **A purchase order to its seller.** 10c's `PurchaseOrder` dialog gains "Approve and send to <seller>", which approves the order and sends one message of purpose `purchase_order` with `orders/PO-<n>.xlsx` attached, the agent's draft text shown first and editable, all in one press, the founder's approval of that one message. "Approve, I'll place it myself" stays.
- **One migration**, at the next free number: `open_seller_drafts` and `open_seller_replies` on the project's projection, for Today's counts.
- **Events:** `mailbox.connected`, `mailbox.disconnected` (with `purpose`, which phase 13's receipts mailbox also uses), `seller_message.drafted`, `seller_message.sent`, `seller_message.failed`, `seller_message.discarded`, `seller_reply.received`, `seller_reply.dismissed`.
- **The kit** gains one skill, `contacting-sellers`: who to write to (the maker or an authorised seller first, a reseller's listing second); what a quote request holds (the item and its exact specification, the quantity, the delivery place and date, the currency, the terms asked for, the reply-by date); one seller per message; never send the business's own data beyond what the quote needs; never promise to buy; plain, polite, short; a follow-up only after five working days and at most once.

## File map

```
docs/design/mockups/contacting-sellers.*                      creates (Task 0)
Cargo.toml, crates/runtime/Cargo.toml                          modifies: lettre, async-imap, mail-parser (Task 1)
crates/runtime/src/mailbox.rs                                  creates: connect, send, fetch replies (Tasks 1, 3, 4)
crates/runtime/src/tools/seller.rs, tools.rs, orchestrator/session.rs   creates/modifies: the three tools (Tasks 2, 5)
crates/runtime/src/orchestrator/human.rs, rules.rs, daemon/gates.rs     modifies: send, discard, the 15-minute check, waiting rows (Tasks 3, 4)
crates/store/src/migrations/<next>_seller_mail.sql, projections.rs, waiting.rs   creates/modifies (Task 3)
docs/schemas/event.schema.json, command.schema.json, rpc.schema.json   modifies (Tasks 1 to 4)
crates/cli/src/procurement.rs                                  creates: mailbox connect, disconnect, check (Task 6)
apps/web/src/pages/ProcurementMailbox.tsx, Today.tsx, dialogs/SellerMessage.tsx, dialogs/PurchaseOrder.tsx (+tests)   creates/modifies (Task 7)
crates/roles/roles/procurement_specialist/skills/contacting-sellers/SKILL.md, kit.yaml, crates/roles/src/kit.rs   creates/modifies (Task 8)
docs/SPEC.md, docs/plans/project-plan.md                       modifies (Task 9)
```

## Interfaces

Consumes: the secret store of step 01 (`credential.rs`); 10c's human-only command check, Today rows, `PurchaseOrder` dialog and `orders/PO-<n>.xlsx`; 10b's folder; `Call`, `offered_tools`, `embedded_skills`.

Produces:

```rust
pub struct MailboxSettings { pub address: String, pub imap_host: String, pub imap_port: u16, pub smtp_host: String,
    pub smtp_port: u16, pub username: String, pub signature: String, pub disclose_ai: bool }   // farik_runtime::mailbox
pub async fn check_login(settings: &MailboxSettings, password: &str) -> Result<(), MailboxError>;
pub async fn send(settings: &MailboxSettings, password: &str, message: &Outgoing) -> Result<String, MailboxError>;  // the Message-ID
pub async fn fetch_replies(settings: &MailboxSettings, password: &str, ledger: &Ledger, sent: &SentIndex) -> Result<Vec<Reply>, MailboxError>;
pub enum MailboxError { NeedsTls, Login, Server(String), TooLarge }
```

## Tasks

### Task 0: Mockups

`ProcurementMailbox` (connect, with the provider table's help), Today's "Messages to sellers" and replies rows, `SellerMessage` (read, edit, send), and `PurchaseOrder`'s "Approve and send", desktop and phone, approved by the founder.

- [ ] `docs(design): mock up contacting sellers`

### Task 1: Connecting a mailbox

Tested against a local IMAP and SMTP fixture (GreenMail's image, pinned by digest, in the Docker-backed integration tests as the Playwright image is).

- `connects_after_logging_in_to_both`: good settings record `mailbox.connected { purpose: procurement }` and keep the password in the store; nothing is sent. RED.
- `refuses_a_server_without_tls`: `mailbox_needs_tls`, nothing kept. RED.
- `a_wrong_password_keeps_nothing`: the login error in Farik's words, the store unchanged. RED.
- `disconnecting_deletes_the_password`. RED.

- [ ] `feat(runtime): connect a procurement mailbox`

### Task 2: Drafting

- `drafts_a_message_to_one_seller`: records `seller_message.drafted`, the text in `mail/out/<n>.txt` and its hash in the event. RED.
- `refuses_each_bad_field`: two addresses, a display name, 8,001 characters, a two-line subject, `purchase_order` on a `question`, an unapproved order; each writes nothing. RED.
- `refuses_another_role_and_the_twenty_first_draft`. RED.

- [ ] `feat(runtime): let the Procurement Specialist draft a message to a seller`

### Task 3: Only the founder sends

- `send_delivers_the_text_with_signature_and_disclosure`: the fixture's SMTP receives exactly the draft, the signature and the disclosure line; `seller_message.sent` carries the `Message-ID`. RED.
- `an_edited_message_sends_the_founders_text`: `edited: true`, the hash of the sent text. RED.
- `an_agent_cannot_send`: an agent session's identity sending is refused and nothing reaches SMTP. RED.
- `a_refused_send_keeps_the_draft`: `seller_message.failed`, the draft still waiting. RED.
- `the_fifty_first_send_of_a_day_is_refused`. RED.

- [ ] `feat(runtime): send a seller message when the founder presses Send`

### Task 4: Replies

- `reads_a_reply_to_a_sent_message`: `In-Reply-To` matching records `seller_reply.received` and keeps text and a PDF; the message stays unread on the server. RED.
- `never_reads_an_unrelated_message`: a message from an address never written to is not fetched past headers and records nothing. RED.
- `skips_a_large_or_foreign_attachment`: an 11 MB PDF and a `.exe` are listed by name, not kept. RED.
- `reads_each_reply_once`: two checks record one event, by the ledger's UID. RED.
- `checks_every_fifteen_minutes_without_a_session`. RED.

- [ ] `feat(runtime): read sellers' replies from the procurement mailbox`

### Task 5: What the agent reads

- `the_agent_reads_sent_text_and_replies_as_untrusted`: the founder's edited text is what `farik_read_seller_messages` gives; a reply's text sits inside the notice. RED.

- [ ] `feat(runtime): let the Procurement Specialist read its messages and replies`

### Task 6: The command line

- `mailbox_connect_reads_the_password_without_echo`; `procurement_check_fetches_now`. RED each.

- [ ] `feat(cli): connect and check the procurement mailbox`

### Task 7: The web app

- `edit_then_send_sends_the_edited_text`; `send_all_sends_each_draft`; `a_reply_offers_ask_for_a_comparison`; `approve_and_send_attaches_the_order`; `the_mailbox_page_fills_a_known_providers_hosts`. RED each.

- [ ] `feat(web): send messages to sellers and read their replies`

### Task 8: The skill

- `procurement_kit_carries_contacting_sellers`: the kit's skills end with `contacting-sellers`. RED.

- [ ] `feat(roles): teach the Procurement Specialist to contact sellers`

### Task 9: Spec and plan

`docs/SPEC.md` 6.10 (as built), 6.6 (the mailbox plumbing phase 13 reuses), 8.4 (`mail/` in the folder), 8.5 (the eight kinds), 8.6 (replies are untrusted; the mailbox's boundary); the revision line. Project plan row 10f and phase 13 step 02's row (it reuses `mailbox.rs`).

- [ ] `docs(spec): record contacting sellers`

## Verification

```
cargo xtask check --integration
# expected: xtask check: ok (with pnpm check, and the mail fixture's tests)
```

Then, by the founder, with a real mailbox: connect it; the Procurement Specialist drafts quote requests to three sellers of one product; edit one and send all three; reply to one from another mailbox; see the reply on Today, ask for a comparison, and see the agent read the reply as untrusted text.

## Execution notes

None yet.
