# 0083. The owner talks to the team on WhatsApp

Date: 2026-10-21
Status: accepted (the founder, 2026-10-21, quoted below; the mechanisms decided by the planner the same day)

The founder: "Create a feature to connect catervas to WhatsApp and be able to send prompts from there."

Asked, the founder answered: how it connects, "Official API, after Catervas Cloud"; what a message does, "Private chat with one agent"; what comes back, "Answers to my messages, Questions waiting on me, Sprint/delivery summaries"; where it sits, "In phase 13 notifications"; and whose number the owner writes to, "Catervas's own number".

Amends ADR 0044 (Catervas Cloud also runs the WhatsApp relay, from phase 13). Builds on ADR 0026 (one-to-one chats), ADR 0047 (hosted work is planned in `catervas-ops`), ADR 0048 (Catervas Cloud is phase 10), ADR 0062 (deliveries) and ADR 0063 (a task from a chat).

## Context

The owner is away from the computer most of the day. Today the only way to give the team a prompt, read an agent's answer or answer a question is the web app on the computer that runs `catervas serve`. That computer has no public address, and nothing on the internet can reach it until Catervas Cloud runs (phase 10).

Three ways to reach WhatsApp were on the table:

- A linked device, the way WhatsApp Web works, through an unofficial library. It needs no public address and no Meta account, but it breaks WhatsApp's terms and puts the owner's number at risk of a ban.
- Meta's WhatsApp Business Platform (the Cloud API) with each owner's own Meta app and a tunnel for its webhook. It is official, but setting it up is far beyond a non-technical owner (ADR 0016), and the tunnel is one more thing that can fail.
- The Cloud API on Catervas Cloud: Catervas's own business number, Meta app and token held server side, with the webhook received by Catervas Cloud's relay and passed to the owner's computer. It is official, takes one step to set up and keeps every secret out of the code (ADR 0043), but it can only come after phase 10.

The founder chose the third.

## Decision

1. **Catervas's number, on Catervas Cloud.** Catervas Cloud runs one WhatsApp Business number on the Cloud API. Its Meta app, access token and webhook secret are held server side, in no build (ADR 0043). The relay that receives Meta's webhooks, its hosting, Catervas's Meta Business verification and the message templates Meta approves are planned in `catervas-ops` (ADR 0047). This repository plans the app's side and holds the relay's API as the contract between the two, as phase 10 step 06 does for sign-ins. No unofficial WhatsApp library is used, now or later.
2. **Linking a number.** Settings gains "WhatsApp". It needs a Catervas account (phase 10 step 05). "Link WhatsApp" shows a six-digit code, good for ten minutes, and a `wa.me` link that opens WhatsApp with "link <code>" already typed to Catervas's number. When the owner sends it, the relay ties that WhatsApp number to the account and to this computer, and Catervas records `whatsapp.linked { number_hint }`, the hint being the number's last four digits only. One number per computer. "Unlink" in Settings, or the message "stop", unties it (`whatsapp.unlinked`). The relay drops a message from any number that is not linked and answers it once a day: "This number isn't linked to a Catervas."
3. **How a message reaches the computer.** `catervas serve` keeps one outbound connection to the relay, opened with the account's session (phase 10 step 05), and receives the owner's messages over it. While the computer is off the relay keeps a message for at most 24 hours and then drops it, answering "Your Catervas has been offline for a day; this message was not delivered." It keeps a message's text only until it is delivered or dropped. The privacy policy (phase 10 step 03) says so.
4. **A message is a private chat message.** A text from the linked number is posted to one agent's one-to-one chat as if typed in the web app: `chat_message_post` gains an optional `via: whatsapp`, which `chat_message.posted` records. That agent's `chat` session answers it under the rules the chat has now, read tier and its one reply (4.3). The agent is the one last written to from WhatsApp, the Product Manager at first. A message that starts with `@<name>` goes to the active agent of that name, matched without regard to case, and later messages go there too until another `@<name>`. A message that starts with `task:` is sent with `as_task: true`, under ADR 0063's rules and refusals; a refusal is sent back in its own words. An unknown name gets "There's no <name> on your team. Your team: <names>." back. Only text is read: a voice note, picture or file gets "Only text messages for now." back.
5. **Answers go back.** A chat reply to a message that came from WhatsApp is also sent to WhatsApp, headed with the agent's name. Its Markdown becomes WhatsApp's formatting (bold, italic, lists, code), and a reply over 4,096 characters is split at paragraph breaks. A reply to a message typed in the web app stays in the web app.
6. **Questions waiting on the owner.** A `question.asked` from `catervas_ask_human` is sent as "CTV-<n> · <agent> asks: <question>". Replying to that message, using WhatsApp's reply to a message, answers it exactly as the web app's answer does; a message that replies to nothing is a chat message (decision 4). Under autopilot no question reaches the owner (ADR 0082), so none is sent.
7. **Delivery summaries.** When a delivery closes (`delivery.closed`, ADR 0062), one message lists the epics and tasks it accepted, by number and title, with a link to the web app's Today.
8. **What is never done from WhatsApp.** Nothing that spends money or changes what the team may do: no approval of a paid or `ask` connector call, a marketing plan that spends, a purchase order, a seller message or a data pipeline; no answer to an escalation; no change to Settings, autopilot, the team or its rules. A message that asks for one of these is a chat message like any other, and the agent can only say where in the web app to do it.
9. **Messages outside the window.** Meta lets a business write freely only within 24 hours of the owner's last message. Outside that window Catervas Cloud sends questions and summaries as templates Meta has approved. A message the relay holds back waits in the web app, and the owner's next message is answered first with "<n> updates are waiting in Catervas." How these messages are paid for and limited is decided and planned in `catervas-ops`, not here.
10. **The owner's switches.** Settings' WhatsApp section lets the owner turn questions and summaries off one by one, both on when linked. Answers to the owner's own messages always go back. Quiet hours, which phase 13 step 03 adds to notifications, hold questions and summaries back until they end.
11. **Where it is built.** Phase 13 step 03b, after notifications, which needs phase 10's account and relay. Its step plan, and the relay API it holds, are written when phase 13 is next planned.

## Consequences

- The owner can give the team work and answer its questions from a phone with no app to install and no Meta account.
- Catervas Cloud now carries the owner's message text for a moment and must say so in its privacy policy. Catervas's number is one account with Meta: if Meta restricts it, every owner's WhatsApp stops at once, and the web app is the fallback.
- A lost or borrowed phone can talk to the team as the owner and file tasks with `task:`. Decision 8 keeps money, the team and its rules out of reach. "Unlink" in Settings cuts it off at once.
- Nothing ships until phases 10 and 13; until then the web app is the only way in.
- New event kinds `whatsapp.linked` and `whatsapp.unlinked`, and the optional `via` on `chat_message_post` and `chat_message.posted`. Every addition is optional on the wire, so every log stays readable.

## Alternatives rejected

- A linked device through an unofficial library: against WhatsApp's terms; the founder chose the official API.
- Each owner's own Meta app and a tunnel: too much setup for a non-technical owner.
- Each owner's own business number through Meta's sign-up flow: the founder chose Catervas's number.
- A message as a post in the Team channel, or always as a new request: the founder chose a private chat.
