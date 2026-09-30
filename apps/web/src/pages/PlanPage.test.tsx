import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FakeSocket } from "../test/fake-socket.ts";
import { CONTRACT, SUMMARY, TEAM } from "../test/plan.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

/** The plan page for FRK-1, with the team, the plan, its checks and its questions answered. */
async function opened() {
	const { container, socket } = await renderApp("/tasks/FRK-1/plan");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "contract.get", { contract: CONTRACT });
	await answerQuery(s, "task.checks", {
		checks: [
			{
				criterion_id: "summary_present",
				text: "The plan has no short summary for you to decide on.",
				passed: false,
				evidence: "summary is missing",
			},
		],
	});
	await answerQuery(s, "questions.list", {
		questions: [
			{
				question_id: 4,
				agent_id: "mira",
				text: "Which amounts?",
				choices: [],
				answer: "$25, $50 and $100",
			},
		],
	});
	return { container, s };
}

/** The command the page sent, once it has sent `count`. */
const sent = (s: FakeSocket, count = 1) =>
	waitFor(() => {
		const c = s.calls("command")[count - 1];
		if (!c) throw new Error(`fewer than ${count} commands were sent`);
		return c;
	});

describe("plan page", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("reads_the_plan_as_a_letter", async () => {
		const { container, s } = await opened();
		expect(
			await screen.findByRole("heading", {
				name: "Approve the plan for Gift cards",
			}),
		).toBeTruthy();
		expect(
			s.calls("query").find((q) => q.params.name === "task.checks")?.params
				.params,
		).toEqual({ task_id: "FRK-1" });

		// The letter: the summary, signed by the Product Manager.
		const letter = screen.getByRole("region", {
			name: "Mira, your Product Manager, wrote this for you",
		});
		expect(within(letter).getByText(SUMMARY)).toBeTruthy();

		// Each part, with the checks that say when it is done.
		const parts = screen.getByRole("list", { name: "The plan in 2 parts" });
		const [first, second] = within(parts).getAllByRole("listitem");
		expect(
			within(first as HTMLElement).getByText(
				"A customer picks $25, $50 or $100, pays, and gets a receipt.",
			),
		).toBeTruthy();
		expect(
			within(first as HTMLElement).getByText(
				"Done when a test purchase of each amount goes through.",
			),
		).toBeTruthy();
		expect(
			within(second as HTMLElement).getByText(
				"Done when a test email arrives with a working code.",
			),
		).toBeTruthy();

		const out = screen.getByRole("list", { name: "Not in this plan" });
		expect(
			within(out)
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual(["Printed gift cards", "Gift cards at the café counter"]);

		const checked = screen.getByRole("region", { name: "What Farik checked" });
		expect(
			within(checked).getByText(
				"The plan has no short summary for you to decide on.",
			),
		).toBeTruthy();
		expect(within(checked).getByText("Not yet")).toBeTruthy();

		// The plan as written, as YAML, on request.
		const written = screen.getByText("See the plan as written");
		expect(written.closest("details")?.open).toBe(false);
		const yaml = container.querySelector("pre")?.textContent ?? "";
		expect(yaml).toContain("exit_criteria:");
		expect(yaml).toContain("  - id: C1");
		expect(yaml).toContain("max_cost_usd: 14");

		const about = screen.getByRole("region", { name: "About this plan" });
		for (const text of [
			"Thursday 24 September",
			"1 answered",
			"Medium",
			"$14.00",
			"Software Developer",
		])
			expect(within(about).getByText(text)).toBeTruthy();
		expect(
			screen
				.getByRole("link", { name: "Edit the plan yourself" })
				.getAttribute("href"),
		).toBe("/tasks/FRK-1/plan/edit");
		await expectNoAxeViolations(container);
	});

	it("approves_or_asks_for_changes", async () => {
		const { container, s } = await opened();
		fireEvent.click(
			await screen.findByRole("button", { name: "Approve the plan" }),
		);
		const approve = await sent(s);
		expect(approve.params).toEqual({
			command: {
				command: "human_accept",
				body: { task_id: "FRK-1", subject: "contract" },
			},
		});
		act(() =>
			s.reply(approve, {
				error: {
					kind: "refused",
					detail: "not_awaiting_approval: the plan is not waiting for you",
				},
			}),
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			"The plan is not waiting for you",
		);

		// Asking for changes needs a note, then sends the plan back.
		fireEvent.click(screen.getByRole("button", { name: "Ask for changes" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Ask Mira for changes",
		});
		const send = within(dialog).getByRole("button", { name: "Send it back" });
		expect((send as HTMLButtonElement).disabled).toBe(true);
		fireEvent.change(within(dialog).getByLabelText(/What should change\?/), {
			target: { value: "Leave out the email part for now." },
		});
		fireEvent.click(send);
		const back = await sent(s, 2);
		expect(back.params).toEqual({
			command: {
				command: "human_send_back",
				body: {
					task_id: "FRK-1",
					subject: "contract",
					message: "Leave out the email part for now.",
					failed_criteria: [],
				},
			},
		});
		await expectNoAxeViolations(container);
	});
});
