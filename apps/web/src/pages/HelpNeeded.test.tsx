import { expectNoAxeViolations } from "@farik/ui/test";
import { act, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { openedGate, sentCommand, TASK } from "../test/gate.ts";

describe("help page", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("offers_the_choices_for_the_reason", async () => {
		const { container, s } = await openedGate(
			"/tasks/FRK-1/help",
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
		).toEqual({ task_id: "FRK-1" });
		const said = screen.getByRole("region", {
			name: "Theo, your Software Developer, explains what happened",
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
					task_id: "FRK-1",
					to: "in_progress",
					extra_tries: 2,
					message: "Wait for the page before the check.",
				},
			},
		});

		act(() =>
			s.reply(more, {
				error: {
					kind: "refused",
					detail: "extra_tries_only_for_tries: more tries resume the work",
				},
			}),
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			"More tries resume the work",
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
					task_id: "FRK-1",
					to: "refining",
					message: "Ask Mira to change the plan",
				},
			},
		});

		const about = screen.getByRole("region", { name: "About this task" });
		for (const text of [
			"07:52 UTC",
			"1 of 3",
			"$1.82 of $14.00",
			"Ada, Architect",
		])
			expect(within(about).getByText(text)).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
