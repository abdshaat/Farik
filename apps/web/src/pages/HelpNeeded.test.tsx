import { expectNoAxeViolations } from "@catervas/ui/test";
import { fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { event, note, openedGate, sentCommand, TASK } from "../test/gate.ts";

const HELP = [
	"team.get",
	"contract.get",
	"task.history",
	"task.tries",
	"escalation.choices",
];
const ESCALATED = { ...TASK, status: "escalated" };
const raised = (seq: number, reason: string, detail: string, by?: string) =>
	event(
		seq,
		"escalation.raised",
		{ reason, detail },
		"2026-09-25T07:52:00Z",
		by,
	);

describe("help page", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("offers_the_choices_for_the_reason", async () => {
		const { container, s } = await openedGate(
			"/tasks/CTV-1/help",
			[
				"team.get",
				"contract.get",
				"task.history",
				"task.tries",
				"escalation.choices",
			],
			{ ...TASK, status: "escalated" },
		);
		expect(
			await screen.findByRole("heading", {
				name: "Theo needs your help with “Gift cards”",
			}),
		).toBeTruthy();
		expect(
			s.calls("query").find((q) => q.params.name === "escalation.choices")
				?.params.params,
		).toEqual({ task_id: "CTV-1" });
		// Catervas stopped the task for its tries, so the words are Catervas's.
		const said = screen.getByRole("region", {
			name: "Catervas explains what happened",
		});
		expect(
			within(said).getByText(
				"The purchase test keeps timing out before the page is ready.",
			),
		).toBeTruthy();
		expect(
			within(screen.getByRole("list", { name: "What Theo tried" }))
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual([
			"Made the receipt show the code.",
			"Ran the purchase test three times.",
		]);

		// The first choice is picked; the button says what it does.
		const now = screen.getByRole("group", { name: "What should happen now?" });
		expect(
			within(now)
				.getAllByRole("radio")
				.map((r) => r.closest("label")?.textContent),
		).toEqual(["Give 2 more tries", "Ask Mira to change the plan"]);
		fireEvent.change(screen.getByLabelText(/A note for Theo/), {
			target: { value: "Wait for the page before the check." },
		});
		fireEvent.click(screen.getByRole("button", { name: "Give 2 more tries" }));
		const more = await sentCommand(s);
		expect(more.params).toEqual({
			command: {
				command: "escalation_resolve",
				body: {
					task_id: "CTV-1",
					to: "in_progress",
					extra_tries: 2,
					message: "Wait for the page before the check.",
				},
			},
		});

		await s.reply(more, {
			error: {
				kind: "refused",
				detail: "extra_tries_only_for_tries: more tries resume the work",
			},
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseExtraTries,
		);

		// Without a note, the choice's label is the message.
		fireEvent.click(within(now).getByLabelText("Ask Mira to change the plan"));
		fireEvent.change(screen.getByLabelText(/A note for Theo/), {
			target: { value: "" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "Ask Mira to change the plan" }),
		);
		expect((await sentCommand(s, 2)).params).toEqual({
			command: {
				command: "escalation_resolve",
				body: {
					task_id: "CTV-1",
					to: "refining",
					message: "Ask Mira to change the plan",
				},
			},
		});

		const about = screen.getByRole("region", { name: "About this task" });
		for (const text of [
			"07:52 UTC",
			"1 of 4",
			"$1.82 of $14.00",
			"Ada, Architect",
		])
			expect(within(about).getByText(text)).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("signs_the_agents_own_words_and_lists_what_it_tried_since", async () => {
		await openedGate("/tasks/CTV-1/help", HELP, ESCALATED, undefined, {
			"task.history": {
				events: [
					note(2, "progress", "Tried before you last answered.", "theo"),
					event(
						3,
						"escalation.resolved",
						{ to: "in_progress" },
						"2026-09-25T07:00:00Z",
					),
					note(4, "progress", "Asked the payment page for its key.", "theo"),
					raised(
						5,
						"explicit_request",
						"I need the payment key to go on.",
						"theo",
					),
				],
			},
		});
		const said = await screen.findByRole("region", {
			name: "Theo, your Developer, explains what happened",
		});
		expect(
			within(said).getByText("I need the payment key to go on."),
		).toBeTruthy();
		expect(
			within(screen.getByRole("list", { name: "What Theo tried" }))
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual(["Asked the payment page for its key."]);
	});

	it("sends_a_plan_to_approve_to_its_page", async () => {
		const { container } = await openedGate(
			"/tasks/CTV-1/help",
			HELP,
			ESCALATED,
			undefined,
			{
				"task.history": {
					events: [raised(5, "approval", "The plan waits.", "mira")],
				},
				"escalation.choices": { choices: [] },
			},
		);
		const read = await screen.findByRole("link", { name: "Read the plan" });
		expect(read.getAttribute("href")).toBe("/tasks/CTV-1/plan");
		expect(screen.queryByLabelText(/A note for Theo/)).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("says_when_there_is_nothing_to_choose", async () => {
		await openedGate("/tasks/CTV-1/help", HELP, ESCALATED, undefined, {
			"task.history": { events: [raised(5, "risk_gate", "Risky work.")] },
			"escalation.choices": { choices: [] },
		});
		expect(
			await screen.findByText("There is nothing to choose here yet."),
		).toBeTruthy();
		// Raised by no agent: Catervas's words.
		expect(
			screen.getByRole("region", { name: "Catervas explains what happened" }),
		).toBeTruthy();
		expect(screen.queryByLabelText(/A note for Theo/)).toBeNull();
	});
});
