import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { CONTRACT, TEAM } from "../test/plan.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

/** The editor for FRK-1, with the team and the plan answered. */
async function opened(contract: object = CONTRACT) {
	const { container, socket } = await renderApp("/tasks/FRK-1/plan/edit");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "contract.get", { contract });
	return { container, s };
}

const checks = (s: FakeSocket) =>
	s.calls("query").filter((q) => q.params.name === "contract.check");
const gets = (s: FakeSocket) =>
	s.calls("query").filter((q) => q.params.name === "contract.get");
/** Moves the faked clock on by `ms`, running what falls due inside `act`. */
const tick = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));

/** The command the page sent, once it has sent `count`. */
const sent = (s: FakeSocket, count = 1) =>
	waitFor(() => {
		const c = s.calls("command")[count - 1];
		if (!c) throw new Error(`fewer than ${count} commands were sent`);
		return c;
	});

describe("plan editor", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("checks_as_you_type", async () => {
		const { container, s } = await opened();
		// The plan as it is, checked once on opening.
		await waitFor(() => expect(checks(s)).toHaveLength(1));
		await s.reply(checks(s)[0] as never, {
			failures: [
				{
					rule: "out_of_scope_present",
					message: "scope.out_of_scope is empty",
					plain: "The plan does not say what it leaves out.",
				},
			],
			total: 6,
		});
		const verdict = await screen.findByRole("region", {
			name: "Ready to approve?",
		});
		await waitFor(() =>
			expect(verdict.textContent).toContain(
				"5 of 6 checks pass. The one left: the plan does not say what it leaves out.",
			),
		);

		// Three keystrokes: one check, 400 ms after the last. The clock is the test's, so a
		// loaded machine neither stretches the wait nor fires the check early.
		vi.useFakeTimers();
		const intent = screen.getByLabelText("Why you want it");
		for (const value of ["C", "Cu", "Cus"])
			fireEvent.change(intent, { target: { value } });
		await tick(399);
		expect(checks(s)).toHaveLength(1);
		await tick(1);
		expect(checks(s)).toHaveLength(2);
		const asked = checks(s)[1]?.params.params as {
			task_id: string;
			contract: { intent: string; exit_criteria: unknown[] };
		};
		expect(asked.task_id).toBe("FRK-1");
		expect(asked.contract.intent).toBe("Cus");
		expect(asked.contract.exit_criteria).toHaveLength(2);
		await tick(500);
		expect(checks(s)).toHaveLength(2);
		vi.useRealTimers();
		await s.reply(checks(s)[1] as never, {
			failures: [
				{
					rule: "schema",
					message: "intent is too short",
					plain: '"Cus" is shorter than 20 characters',
				},
				{
					rule: "schema",
					message: "summary is too short",
					plain: '"x" is shorter than 20 characters',
				},
			],
			total: 1,
		});
		await waitFor(() =>
			expect(verdict.textContent).toContain("0 of 1 checks pass."),
		);
		expect(within(verdict).getAllByRole("listitem")).toHaveLength(2);
		await expectNoAxeViolations(container);
	});

	it("locks_and_saves_back_to_refining", async () => {
		const { container, s } = await opened();
		expect(
			await screen.findByText("Mira can still change this plan"),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Lock the plan" }));
		const lock = await sent(s);
		expect(lock.params).toEqual({
			command: { command: "contract_lock", body: { task_id: "FRK-1" } },
		});
		await s.reply(lock, { said: "locked", events: [30] });
		// The page reads the plan again, now locked.
		await waitFor(() => expect(gets(s)).toHaveLength(2));
		await s.reply(gets(s)[1] as never, {
			contract: {
				...CONTRACT,
				locked: true,
				updated_at: "2026-09-24T10:05:00Z",
			},
		});
		expect(await screen.findByText("You have locked this plan")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Unlock" }));
		const unlock = await sent(s, 2);
		expect(unlock.params).toEqual({
			command: { command: "contract_unlock", body: { task_id: "FRK-1" } },
		});
		await s.reply(unlock, { said: "unlocked", events: [31] });
		await waitFor(() => expect(gets(s)).toHaveLength(3));
		await s.reply(gets(s)[2] as never, {
			contract: { ...CONTRACT, updated_at: "2026-09-24T10:06:00Z" },
		});
		await screen.findByText("Mira can still change this plan");

		// Saving sends the plan as edited; a frozen plan goes back to refining.
		fireEvent.change(screen.getByLabelText("Why you want it"), {
			target: { value: "Customers can send a gift card to a friend by email." },
		});
		fireEvent.click(screen.getByRole("button", { name: "Save" }));
		const save = await waitFor(() => {
			const c = s.calls("contract.save")[0];
			if (!c) throw new Error("nothing was saved");
			return c;
		});
		const params = save.params as {
			task_id: string;
			contract: { intent: string; locked: boolean; budget: object };
		};
		expect(params.task_id).toBe("FRK-1");
		expect(params.contract.intent).toBe(
			"Customers can send a gift card to a friend by email.",
		);
		expect(params.contract.locked).toBe(false);
		expect(params.contract.budget).toEqual({ max_cost_usd: 14 });
		await s.reply(save, { saved: true, back_to_refining: true });
		expect(
			await screen.findByText(
				"Saved. Mira checks the plan again, then it comes back to you to approve.",
			),
		).toBeTruthy();
		// A save refuses stale fields, so the page reads the plan again.
		await waitFor(() => expect(gets(s)).toHaveLength(4));
		await expectNoAxeViolations(container);
	});

	it("keeps_what_is_typed_as_a_fresh_read_lands", async () => {
		const { s } = await opened();
		fireEvent.click(
			await screen.findByRole("button", { name: "Lock the plan" }),
		);
		await s.reply(await sent(s), { said: "locked", events: [30] });
		await waitFor(() => expect(gets(s)).toHaveLength(2));
		await s.reply(gets(s)[1] as never, {
			contract: { ...CONTRACT, locked: true },
		});
		fireEvent.click(await screen.findByRole("button", { name: "Unlock" }));
		await s.reply(await sent(s, 2), { said: "unlocked", events: [31] });
		await waitFor(() => expect(gets(s)).toHaveLength(3));
		// The person types the moment the unlocked read shows, before React's effects have run:
		// the read is answered outside `act`, and the keystroke comes from the observer that
		// first sees it on the page.
		const mine = "Customers can send a gift card to a friend by email.";
		const typed = new Promise<void>((done) => {
			const seen = new MutationObserver(() => {
				if (!screen.queryByText("Mira can still change this plan")) return;
				seen.disconnect();
				fireEvent.change(screen.getByLabelText("Why you want it"), {
					target: { value: mine },
				});
				done();
			});
			seen.observe(document.body, {
				childList: true,
				characterData: true,
				subtree: true,
			});
		});
		const read = gets(s)[2] as { id: number };
		s.emit("message", {
			data: JSON.stringify({
				jsonrpc: "2.0",
				id: read.id,
				result: { contract: CONTRACT },
			}),
		});
		await typed;
		await act(async () => {});
		expect(
			(screen.getByLabelText("Why you want it") as HTMLTextAreaElement).value,
		).toBe(mine);
	});

	it("holds_the_work_before_changing_a_plan_the_team_works_to", async () => {
		const { s } = await opened({ ...CONTRACT, status: "in_progress" });
		fireEvent.click(await screen.findByRole("button", { name: "Save" }));
		const save = await waitFor(() => {
			const c = s.calls("contract.save")[0];
			if (!c) throw new Error("nothing was saved");
			return c;
		});
		await s.fail(
			save,
			-32005,
			"the team is working to this plan; hold the work first, then change it",
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			"The team is working to this plan; hold the work first, then change it",
		);
		fireEvent.click(screen.getByRole("button", { name: "Hold the work" }));
		const hold = await sent(s);
		expect(hold.params).toEqual({
			command: {
				command: "task_transition",
				body: {
					task_id: "FRK-1",
					to: "escalated",
					reason: "Held by you to change the plan",
				},
			},
		});
	});

	it("keeps_your_draft_when_the_plan_is_read_again", async () => {
		const { container, s } = await opened({
			...CONTRACT,
			status: "in_progress",
		});
		const mine = "Customers can send a gift card to a friend by email.";
		fireEvent.change(await screen.findByLabelText("Why you want it"), {
			target: { value: mine },
		});
		// Holding the work moves the task, which stamps the file: the page reads it again.
		fireEvent.click(screen.getByRole("button", { name: "Hold the work" }));
		await s.reply(await sent(s), {
			said: "held",
			events: [30],
		});
		await waitFor(() => expect(gets(s)).toHaveLength(2));
		await s.reply(gets(s)[1] as never, {
			contract: {
				...CONTRACT,
				status: "escalated",
				updated_at: "2026-09-24T10:05:00Z",
			},
		});
		await screen.findByText("Mira can still change this plan");
		expect(
			(screen.getByLabelText("Why you want it") as HTMLTextAreaElement).value,
		).toBe(mine);
		expect(
			screen.queryByRole("button", { name: "Take the new version" }),
		).toBeNull();

		// Mira writes the plan while you type: a field only she changed is hers, one you both
		// changed stays yours, and the page says so.
		const hers = "Customers will be able to buy a gift card and send it on.";
		act(() => s.event(31));
		await waitFor(() => expect(gets(s)).toHaveLength(3));
		await s.reply(gets(s)[2] as never, {
			contract: {
				...CONTRACT,
				status: "escalated",
				intent: "Customers can give a gift card and use it.",
				summary: hers,
				updated_at: "2026-09-24T10:07:00Z",
			},
		});
		expect(
			await screen.findByText(
				"Mira changed this plan while you were editing. Your changes are kept.",
			),
		).toBeTruthy();
		expect(
			(screen.getByLabelText("Why you want it") as HTMLTextAreaElement).value,
		).toBe(mine);
		expect(
			(
				screen.getByLabelText(
					"The summary you read first",
				) as HTMLTextAreaElement
			).value,
		).toBe(hers);

		// A save sends your field over the plan as last read, its status among it.
		fireEvent.click(screen.getByRole("button", { name: "Save" }));
		const save = await waitFor(() => {
			const c = s.calls("contract.save")[0];
			if (!c) throw new Error("nothing was saved");
			return c;
		});
		const saved = (save.params as { contract: Record<string, unknown> })
			.contract;
		expect([saved.intent, saved.summary, saved.status]).toEqual([
			mine,
			hers,
			"escalated",
		]);

		// Or take her version whole.
		fireEvent.click(
			screen.getByRole("button", { name: "Take the new version" }),
		);
		expect(
			(screen.getByLabelText("Why you want it") as HTMLTextAreaElement).value,
		).toBe("Customers can give a gift card and use it.");
		expect(screen.queryByText(/while you were editing/)).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("saves_in_place_while_refining", async () => {
		const { s } = await opened({ ...CONTRACT, status: "refining" });
		// Without Advanced, no check can become a command Farik runs.
		expect(
			await screen.findAllByRole("radio", { name: en.criterionReview }),
		).not.toHaveLength(0);
		expect(
			screen.queryByRole("radio", { name: en.criterionCommand }),
		).toBeNull();

		fireEvent.change(screen.getByLabelText(en.fieldBudget), {
			target: { value: "20" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Save" }));
		const save = await waitFor(() => {
			const c = s.calls("contract.save")[0];
			if (!c) throw new Error("nothing was saved");
			return c;
		});
		// The budget goes as a number.
		expect(
			(save.params as { contract: { budget: object } }).contract.budget,
		).toEqual({ max_cost_usd: 20 });
		await s.reply(save, { saved: true, back_to_refining: false });
		expect((await screen.findByRole("status")).textContent).toBe(en.saved);
	});
});
