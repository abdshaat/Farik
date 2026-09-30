import { expectNoAxeViolations } from "@farik/ui/test";
import { act, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { openedGate, sentCommand } from "../test/gate.ts";

describe("send back dialog", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("sends_back_with_a_note", async () => {
		const { container, s } = await openedGate("/tasks/FRK-1/accept", [
			"team.get",
			"contract.get",
			"task.history",
			"task.checks",
			"task.diff",
			"task.tries",
			"waiting.list",
		]);
		fireEvent.click(
			await screen.findByRole("button", { name: "Send back with a note" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Send Gift cards back to Theo",
		});
		const what = within(dialog).getByRole("group", {
			name: "What is not right? Choose any that apply.",
		});
		expect(
			within(what)
				.getAllByRole("checkbox")
				.map((c) => c.closest("label")?.textContent),
		).toEqual([
			"A test purchase of each amount goes through.",
			"A test email arrives with a working code.",
			"Something else",
		]);
		expect(
			within(dialog).getByText(
				"This is try 1 of 4. After the last, Farik stops and asks you what to do.",
			),
		).toBeTruthy();

		// The note is required.
		const send = within(dialog).getByRole("button", {
			name: "Send back to Theo",
		});
		expect((send as HTMLButtonElement).disabled).toBe(true);
		fireEvent.click(
			within(what).getByLabelText("A test email arrives with a working code."),
		);
		fireEvent.click(within(what).getByLabelText("Something else"));
		fireEvent.change(within(dialog).getByLabelText(/Your note to Theo/), {
			target: { value: "The email never came." },
		});
		fireEvent.click(send);
		const back = await sentCommand(s);
		expect(back.params).toEqual({
			command: {
				command: "human_send_back",
				body: {
					task_id: "FRK-1",
					subject: "result",
					message: "The email never came.",
					failed_criteria: ["C2"],
				},
			},
		});
		act(() =>
			s.reply(back, {
				error: {
					kind: "refused",
					detail:
						"review_first: the reviewer has not finished; send back once the review is in",
				},
			}),
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			"The reviewer has not finished; send back once the review is in",
		);
		await expectNoAxeViolations(container);
	});

	it("sends_something_else_as_no_criterion", async () => {
		const { s } = await openedGate("/tasks/FRK-1/accept", [
			"team.get",
			"contract.get",
			"task.history",
			"task.checks",
			"task.diff",
			"task.tries",
			"waiting.list",
		]);
		fireEvent.click(
			await screen.findByRole("button", { name: "Send back with a note" }),
		);
		const dialog = await screen.findByRole("dialog");
		fireEvent.click(within(dialog).getByLabelText("Something else"));
		fireEvent.change(within(dialog).getByLabelText(/Your note to Theo/), {
			target: { value: "The colours are wrong." },
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Send back to Theo" }),
		);
		// The schema needs the list, empty or not.
		expect((await sentCommand(s)).params).toEqual({
			command: {
				command: "human_send_back",
				body: {
					task_id: "FRK-1",
					subject: "result",
					message: "The colours are wrong.",
					failed_criteria: [],
				},
			},
		});
	});
});
