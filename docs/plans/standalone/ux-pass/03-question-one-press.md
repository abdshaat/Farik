# Standalone UX pass, plan 3: a question answered with one press

Status: ready
Branch: `feat/ux-pass` (work outside a phase, its own pull request to `main`)
Spec: `docs/SPEC.md` 4.2 and 4.3 (answering a question in the web app); recorded by plan 8 (revision 0.123)
Depends on: nothing on this branch; `Questions.tsx`, `InfoTip` (`@catervas/ui`), `Bullets` (`apps/web/src/components/Bullets.tsx`), on main
Readiness confirmed by: the planner itself, 2026-10-17 (ADR 0065)

Signatures, not bodies; test names and what each asserts, not test code; around 300 lines at most (ADR 0008).

## Goal

On a task's question page (screen 17), each answer the agent suggests is a button that sends that answer at once; the owner no longer picks a radio and then presses "Send answer". Writing one's own answer keeps its box and its "Send answer"; "Let Iris decide" stays. The agent's question reads in normal weight, one paragraph per paragraph it wrote, not as one bold block. The "About questions" section under the card becomes an ⓘ beside the page's title, as two bullets. Out of scope: the page's headings and lead lines (`oneQuestionTitle`, `oneQuestionLead`, `questionsTitle`, `questionsLead`, `youAnswered`, `answered`, `laterQuestion`), which `fix/open-issues` (ADR 0069) rewords for answered questions; the help page of a stuck task, whose choices are already one press (SPEC 5.7) and are `fix/open-issues`'s.

## Decisions

- `Answer`'s `Choice` radio group is replaced by a list of buttons: a `<p id="suggests-{questionId}" className={styles.muted}>` with `suggests` "{agent} suggests", then `<ul aria-labelledby="suggests-{questionId}" className={own.choices}>`, one `<li>` per choice holding a `Button` (kind `secondary`) whose text is the choice's label and, when the choice has a hint, a `<p id="hint-{questionId}-{n}" className={styles.muted}>` that the button names in `aria-describedby`. Pressing it calls `send(choice.label)`. Rejected: keeping the radios with an auto-send on change (a keyboard user's arrow keys would send).
- While a send is out, every button of the card is `busy` (the existing `busy` state), so a double press sends once.
- Own words: `TextArea` with `ownWordsOne` / `ownWordsMany` as now; "Send answer" (`primary`) sends `words.trim()` and is disabled while the box is empty. The `choice` state and the `wordsWin` hint go: with no picked choice there is nothing for words to win over. `wordsWin` is deleted from `en.ts`.
- "Let {agent} decide" (`secondary`) stays beside "Send answer" and sends `decideText` as now.
- The question text: `<div className={own.text}>` with one `<p>` per paragraph of `question.text` split on blank lines (`/\n\s*\n/`), normal weight. Rejected: clamping it; the owner must read the whole question to answer it.
- "About questions": on the one-question page the `<section aria-labelledby="about">` with `aboutQuestions`, `aboutQuestionsBody` and `aboutQuestionsNext` is removed; the `<h1>` sits in a `div.titled` (`pages.module.css`) with `<InfoTip id="questions-info"><Bullets text={t("aboutQuestionsTips", { agent: name })} /></InfoTip>`. New key `aboutQuestionsTips`: "A question is not a problem: {agent} just needs your answer.\nYour answer goes straight into {agent}’s next session." `aboutQuestions`, `aboutQuestionsBody` and `aboutQuestionsNext` are deleted. The several-questions page is left as it is (it had no about section, and its heading block is `fix/open-issues`'s to reword).
- Layout: `own.choices` is a grid, gap `--catervas-space-2`; each `li` a grid with gap `--catervas-space-1`; the button `justify-self: start`, and below 640 px `justify-self: stretch` so each answer is a full-width 44 px target on a phone.

## File map

```
apps/web/src/pages/Questions.tsx         modifies: choice buttons, own words, the question's paragraphs, the ⓘ
apps/web/src/pages/Questions.module.css  modifies: .choices, its phone rule; .text loses nothing (it held no weight; the <strong> goes)
apps/web/src/strings/en.ts               modifies: aboutQuestionsTips added; aboutQuestions, aboutQuestionsBody, aboutQuestionsNext, wordsWin deleted
apps/web/src/pages/Questions.test.tsx    tests:   answers_by_choice_or_in_words rewritten; lets_the_agent_decide keeps passing
apps/web/e2e/request.spec.ts             tests:   the answer is one press on "Leave it empty"
```

## Interfaces

Consumes: `Button`, `InfoTip`, `TextArea` (`@catervas/ui`), `Bullets` (`components/Bullets.tsx`), `styles.titled` (`pages.module.css`).

Produces: nothing exported; `Answer`'s props are unchanged (`question`, `name`, `own`).

## Tasks

### Task 1: one press per suggested answer

Files: modified `apps/web/src/pages/Questions.tsx`, `apps/web/src/pages/Questions.module.css`, `apps/web/src/strings/en.ts`, `apps/web/src/pages/Questions.test.tsx`, `apps/web/e2e/request.spec.ts`
Consumes: nothing from earlier plans

Tests, each watched to fail for the stated reason before the code that satisfies it:

- `answers_by_choice_or_in_words` (rewritten in place) — the same three questions. Asserts: the radio group is gone (`queryAllByRole("radio")` is empty); a list named "Mira suggests" holds two buttons, "Never" and "After a year"; "Never"'s accessible description is "Cards keep their value forever."; pressing "After a year" sends at once `question_answer` with `{ question_id: 12, answer: "After a year" }` (no "Send answer" press); while that command is out, "Never" is disabled (busy); after the reply, "Send answer" is disabled while the words box is empty, and typing "Only after five years." then pressing it sends `{ question_id: 12, answer: "Only after five years." }`; the refusal `already_answered` is shown as `en.refuseAlreadyAnswered` in an alert, as now; no axe violations. Fails first because the choices are radios and nothing is sent until "Send answer".
- `lets_the_agent_decide` (kept) — still passes; add one assertion: the text "A question is not a problem: Mira just needs your answer." is only inside a tip (`onlyInTips` from `apps/web/src/test/tips.ts`), and no heading "About questions" exists. Fails first because the about section shows the old words.
- `e2e/request.spec.ts` lines 131-132: replace the radio check and "Send answer" click with one `await page.getByRole("button", { name: /Leave it empty/ }).click();`. The rest of the journey (the poll that the answer was recorded) is unchanged.

- [x] `feat(web): a suggested answer to a question sends with one press`

## Verification

```
pnpm --filter @catervas/web test -- Questions
# expected: pass
pnpm --filter @catervas/web e2e -- request.spec.ts
# expected: all pass
pnpm check
# expected: exit 0
cargo xtask check
# expected: xtask check: ok
```
