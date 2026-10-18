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

	it("answers_with_one_press_and_says_what_each_choice_does", async () => {
		const { container, s } = await openedGate(
			"/tasks/CTV-1/help",
			HELP,
			ESCALATED,
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

		const items = within(
			screen.getByRole("list", { name: "What should happen now?" }),
		).getAllByRole("listitem");
		expect(items).toHaveLength(3);
		const labels = [
			"Give 2 more tries",
			"Ask Mira to change the plan",
			"Cancel the task",
		];
		const nexts = [
			"Theo picks up CTV-1 again with 2 more tries.",
			"Mira rewrites the plan and asks you to approve it again.",
			"CTV-1 stops and nothing more is spent on it.",
		];
		items.forEach((li, n) => {
			const button = within(li).getByRole("button", {
				name: labels[n] as string,
			});
			expect(
				button.getAttribute("aria-describedby") &&
					document.getElementById(
						button.getAttribute("aria-describedby") as string,
					)?.textContent,
			).toBe(nexts[n]);
			expect(within(li).queryByText("Suggested") !== null).toBe(n === 0);
		});
		expect(screen.queryAllByRole("radio")).toHaveLength(0);
		expect(screen.queryAllByRole("combobox")).toHaveLength(0);
		expect(screen.getAllByRole("textbox").map((t) => t.id)).toEqual([
			"help-note",
		]);

		fireEvent.change(screen.getByLabelText("Add a note for Theo (optional)"), {
			target: { value: "Try the slow network." },
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
					message: "Try the slow network.",
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

		fireEvent.change(screen.getByLabelText("Add a note for Theo (optional)"), {
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

	it("opens_the_page_an_open_choice_names", async () => {
		const { container, s } = await openedGate(
			"/tasks/CTV-1/help",
			HELP,
			ESCALATED,
			undefined,
			{
				"task.history": {
					events: [raised(5, "approval", "The plan waits.", "mira")],
				},
				"escalation.choices": {
					choices: [
						{
							label: "Review the plan",
							next: "You read the plan and decide on its page.",
							suggested: true,
							kind: "open",
							path: "/tasks/CTV-1/plan",
						},
						{
							label: "Cancel the task",
							next: "CTV-1 stops.",
							suggested: false,
							kind: "command",
							body: {
								command: "escalation_resolve",
								body: { task_id: "CTV-1", to: "cancelled" },
							},
						},
					],
				},
			},
		);
		const read = await screen.findByRole("link", { name: "Review the plan" });
		expect(read.getAttribute("href")).toBe("/tasks/CTV-1/plan");
		fireEvent.click(read);
		expect(s.calls("command")).toHaveLength(0);
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
