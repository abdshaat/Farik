---
name: plain-language-interface-wording
description: Use when you write or change any words a person reads on a screen, from labels, buttons and headings to help text, empty states and errors.
---

# Plain words on a screen

The person using the screen may not be technical. Every word on it should make sense to them
without a glossary.

## The rules

- **Sentence case** for headings, labels and buttons: "Save changes", not "Save Changes".
- **A verb on every button**, saying what happens: "Send invite", not "OK" or "Submit".
- **No jargon.** No internal names, ids, states, error codes or stack traces on the screen. If a
  technical word is unavoidable, the screen explains it where it appears.
- **Short.** One idea per sentence. Cut what the person does not need to act.
- **Errors say what to do.** Say what went wrong in their terms and the next step: "The file is too
  large. Choose one under 10 MB." Never blame the person.
- **Empty states help.** An empty list says what will appear there and how to add the first one.
- **One word for one idea**, the same across every screen.

## Where the words live

If the project keeps its strings in one place (a strings or translations file), put new words
there and nowhere else. Read the existing strings first, and match their voice.
