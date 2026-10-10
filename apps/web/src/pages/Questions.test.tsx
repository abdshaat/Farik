import { expectNoAxeViolations } from "@catervas/ui/test";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const TEAM = {
	name: "Corner Bakery",
	agents: [
		{
			id: "mira",
			display_name: "Mira",
			role: "product_manager",
			avatar: "product-manager",
			status: "active",
		},
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};
const question = (
	question_id: number,
	text: string,
	answer: string | null,
	choices: object[] = [],
) => ({
	question_id,
	task_id: "CTV-3",
	agent_id: "mira",
	text,
	choices,
	answer,
});

/** The questions page for CTV-3, with the team and `questions` answered. */
async function opened(questions: object[]) {
	const { container, socket } = await renderApp("/tasks/CTV-3/questions");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "questions.list", { questions });
	return { container, s };
}

/** The `n`th command sent, once it has been. */
async function command(s: FakeSocket, n: number) {
	return waitFor(() => {
		const c = s.calls("command")[n];
		if (!c) throw new Error(`no command ${n} was sent`);
		return c;
	});
}

describe("questions page", () => {
	it("answers_by_choice_or_in_words", async () => {
		const { container, s } = await opened([
			question(
				11,
				"Which amounts should a gift card come in?",
				"$25, $50 and $100",
			),
			question(12, "Should a gift card ever run out?", null, [
				{ label: "Never", hint: "Cards keep their value forever." },
				{ label: "After a year" },
			]),
			question(13, "Who can see how much is left on a card?", null),
		]);
		expect(
			await screen.findByRole("heading", { name: "Mira’s questions" }),
		).toBeTruthy();
		expect(
			s.calls("query").find((q) => q.params.name === "questions.list")?.params
				.params,
		).toEqual({ task_id: "CTV-3" });
		expect(screen.getByText("$25, $50 and $100")).toBeTruthy();
		expect(screen.getByText("Cards keep their value forever.")).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("radio", { name: "After a year" }));
		fireEvent.click(screen.getByRole("button", { name: en.sendAnswer }));
		const byChoice = await command(s, 0);
		expect(byChoice.params).toEqual({
			command: {
				command: "question_answer",
				body: { question_id: 12, answer: "After a year" },
			},
		});
		await s.reply(byChoice, { said: "answered", events: [20] });

		const words = screen.getByRole("textbox", { name: en.ownWordsMany });
		expect(screen.queryByText(en.wordsWin)).toBeNull();
		fireEvent.change(words, { target: { value: "Only after five years." } });
		// The choice is still picked: the page says the words go instead.
		expect(words.getAttribute("aria-describedby")).toBeTruthy();
		expect(screen.getByText(en.wordsWin)).toBeTruthy();
		// Once the first answer is taken, the button is free again.
		fireEvent.click(await screen.findByRole("button", { name: en.sendAnswer }));
		const byWords = await command(s, 1);
		expect(byWords.params).toEqual({
			command: {
				command: "question_answer",
				body: { question_id: 12, answer: "Only after five years." },
			},
		});
		// A refused answer is said, in a sentence.
		await s.reply(byWords, {
			error: {
				kind: "refused",
				detail: "already_answered: the question has an answer already",
			},
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseAlreadyAnswered,
		);
	});

	it("lets_the_agent_decide", async () => {
		const { container, s } = await opened([
			question(21, "Should the launch post mention the holiday hours?", null, [
				{ label: "Yes" },
				{ label: "No" },
			]),
		]);
		// One open question is shown on its own.
		expect(
			await screen.findByRole("heading", { name: "Mira has a question" }),
		).toBeTruthy();
		expect(screen.getByText("Mira, your Product Manager, asks")).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("button", { name: "Let Mira decide" }));
		expect((await command(s, 0)).params).toEqual({
			command: {
				command: "question_answer",
				body: {
					question_id: 21,
					answer: "Decide as you think best, and say what you chose.",
				},
			},
		});
	});

	it("hides_later_questions", async () => {
		const { container } = await opened([
			question(31, "Which amounts should a gift card come in?", null),
			question(32, "Should a gift card ever run out?", null),
			question(33, "Who can see how much is left on a card?", null),
		]);
		expect(
			await screen.findByText("Which amounts should a gift card come in?"),
		).toBeTruthy();
		expect(screen.queryByText("Should a gift card ever run out?")).toBe(null);
		expect(screen.queryByText("Who can see how much is left on a card?")).toBe(
			null,
		);
		expect(
			screen.getAllByText("Mira shows this once you answer question 1."),
		).toHaveLength(2);
		await expectNoAxeViolations(container);
	});
});
