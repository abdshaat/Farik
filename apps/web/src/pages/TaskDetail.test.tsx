import { expectNoAxeViolations } from "@farik/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
	COMPLETION,
	HISTORY,
	openedGate,
	REVIEW,
	sentCommand,
	TASK,
} from "../test/gate.ts";

const PAGE = [
	"team.get",
	"contract.get",
	"task.history",
	"task.checks",
	"task.diff",
	"task.tries",
	"waiting.list",
	"team.activity",
	"task.costs",
];
const CONTRACT = {
	...TASK,
	sprint: "S2",
	assignee: "theo",
	reviewer: "ada",
	exit_criteria: [
		...TASK.exit_criteria,
		{
			id: "C3",
			text: "Mira agrees the page reads clearly.",
			verification: { method: "review", rubric: ["It reads clearly."] },
		},
	],
	notes: {
		completion: "Made the cards and the receipt.",
		review: "Checked each amount.",
	},
};
const COSTS = {
	by_purpose: [
		{ words: "Planning", usd: 0.41 },
		{ words: "Building", usd: 1.36 },
		{ words: "Checking", usd: 0.22 },
	],
	total_usd: 1.99,
	limit_usd: 4,
};
const moved = (seq: number, to: string, by: string) => ({
	seq,
	recorded_at: "2026-09-24T11:00:00Z",
	team_id: "t",
	project_id: "p",
	task_id: "FRK-1",
	kind: "task.transitioned",
	body: { from: "ready", to, requested_by: by },
});
const integration = {
	task_id: "FRK-1",
	kind: "integration",
	agent_id: null,
	title: "Gift cards",
	line: "Farik could not add it to your project",
};

/** The task page for FRK-1, with `contract`, `waiting` and any other answer `overrides` gives. */
const opened = (
	contract: object = CONTRACT,
	waiting: object[] = [],
	overrides: Record<string, unknown> = {},
) =>
	openedGate("/tasks/FRK-1", PAGE, contract, waiting, {
		"task.checks": {
			checks: [
				{ criterion_id: "C1", text: "", passed: true, evidence: "3 passed" },
				{ criterion_id: "C2", text: "", passed: false, evidence: "no mail" },
			],
		},
		"team.activity": { activity: [] },
		"task.costs": COSTS,
		...overrides,
	});

describe("task detail", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
	});

	it("shows_the_five_tabs", async () => {
		const { container } = await opened(CONTRACT, [], {
			"task.history": {
				events: [...HISTORY, moved(11, "in_progress", "governor")],
			},
		});
		expect(
			await screen.findByRole("heading", { level: 1, name: "Gift cards" }),
		).toBeTruthy();
		expect(
			screen.getByText(
				"FRK-1 in sprint 2. Theo is doing it, and Ada reviews it. Try 1 of 4.",
			),
		).toBeTruthy();
		const tabs = screen.getAllByRole("tab");
		expect(tabs.map((tab) => tab.textContent)).toEqual([
			"Summary and checks",
			"History",
			"The plan",
			"Code changes",
			"Notes",
		]);
		const panel = () => screen.getByRole("tabpanel");

		// Summary and checks: what it is for, the latest summary signed, and each check's word.
		expect(within(panel()).getByText(TASK.intent)).toBeTruthy();
		expect(
			within(panel()).getByText("Ada, your Architect, reviewed it"),
		).toBeTruthy();
		const checks = within(panel()).getAllByRole("listitem");
		expect(checks.map((c) => c.textContent)).toEqual([
			"A test purchase of each amount goes through.Passed",
			"A test email arrives with a working code.Failed last time",
			"Mira agrees the page reads clearly.Not run yet",
		]);
		await expectNoAxeViolations(container);

		// History: newest first, in plain words, each with its log kind.
		fireEvent.click(screen.getByRole("tab", { name: "History" }));
		const lines = within(panel()).getAllByRole("listitem");
		expect(lines).toHaveLength(HISTORY.length + 1);
		expect(lines[0]?.textContent).toContain("Farik moved it to In progress.");
		expect(lines[0]?.textContent).toContain("task.transitioned");
		expect(lines.at(-1)?.textContent).toContain("You asked for it.");
		expect(lines.at(-1)?.textContent).toContain("task.created");
		expect(within(panel()).getByText("You approved the plan.")).toBeTruthy();

		// The plan: approval and lock, scope, out of scope, risk, and the limit.
		fireEvent.click(screen.getByRole("tab", { name: "The plan" }));
		expect(
			within(panel()).getByText("Approved on Thursday 24 September."),
		).toBeTruthy();
		expect(within(panel()).getByText(/not locked yet/)).toBeTruthy();
		expect(within(panel()).getByText("Gift cards bought online")).toBeTruthy();
		expect(within(panel()).getByText("Printed gift cards")).toBeTruthy();
		expect(within(panel()).getByText("Medium")).toBeTruthy();
		expect(within(panel()).getByText("$14.00 for this task")).toBeTruthy();

		// Code changes: the size, the branch, and the diff.
		fireEvent.click(screen.getByRole("tab", { name: "Code changes" }));
		expect(
			within(panel()).getByText(
				"3 files, +142 −18, on the branch feature/FRK-1.",
			),
		).toBeTruthy();
		expect(
			within(panel()).getAllByText(/src\/gift\.ts/).length,
		).toBeGreaterThan(0);

		// Notes: the latest note of each kind the team wrote in the log, signed by who wrote it,
		// over the contract's own (the agents' notes are events; the contract's are hand-written).
		fireEvent.click(screen.getByRole("tab", { name: "Notes" }));
		expect(
			within(panel()).getByText("Theo, your Developer, wrote this for you"),
		).toBeTruthy();
		expect(within(panel()).getByText(COMPLETION)).toBeTruthy();
		expect(
			within(panel()).getByText("I did not change how prices are worked out."),
		).toBeTruthy();
		expect(within(panel()).getByText(REVIEW)).toBeTruthy();
		expect(within(panel()).queryByText("Checked each amount.")).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("shows_cost_by_purpose", async () => {
		const { container } = await opened();
		const cost = await screen.findByRole("region", { name: "Cost so far" });
		const rows = within(cost)
			.getAllByRole("row")
			.map((r) => r.textContent);
		expect(rows).toEqual([
			"Planning$0.41",
			"Building$1.36",
			"Checking$0.22",
			"Total, of a $4.00 limit$1.99",
		]);
		await expectNoAxeViolations(container);
	});

	it("offers_add_stop_and_cancel_when_they_apply", async () => {
		// In review, with nobody working: no add, no stop; cancel asks for a reason.
		let page = await opened();
		const adding = await screen.findByRole("region", {
			name: "Adding it to your project",
		});
		expect(
			within(adding).getByText(/^Not yet: the task has not been accepted/),
		).toBeTruthy();
		expect(
			screen.queryByRole("button", { name: "Add to the project" }),
		).toBeNull();
		expect(
			screen.queryByRole("button", { name: "Stop work on this task" }),
		).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: "Cancel this task" }));
		const dialog = screen.getByRole("dialog");
		const cancel = within(dialog).getByRole("button", {
			name: "Cancel this task",
		}) as HTMLButtonElement;
		expect(cancel.disabled).toBe(true);
		fireEvent.change(within(dialog).getByLabelText(/Why/), {
			target: { value: "Not needed any more." },
		});
		expect(cancel.disabled).toBe(false);
		await expectNoAxeViolations(page.container);
		fireEvent.click(cancel);
		expect((await sentCommand(page.s)).params).toEqual({
			command: {
				command: "task_transition",
				body: {
					task_id: "FRK-1",
					to: "cancelled",
					reason: "Not needed any more.",
				},
			},
		});
		cleanup();

		// While a session runs on it: stop sends that session.
		page = await opened(CONTRACT, [], {
			"team.activity": {
				activity: [
					{
						agent_id: "theo",
						state: "working",
						line: "Theo is building Gift cards",
						task_id: "FRK-1",
						session_id: "s-7",
						purpose: "implement",
					},
				],
			},
		});
		fireEvent.click(
			await screen.findByRole("button", { name: "Stop work on this task" }),
		);
		expect((await sentCommand(page.s)).params).toEqual({
			command: { command: "session_stop", body: { session_id: "s-7" } },
		});
		cleanup();

		// Accepted and awaiting integration: add, and no cancel.
		page = await opened({ ...CONTRACT, status: "accepted" }, [integration]);
		fireEvent.click(
			await screen.findByRole("button", { name: "Add to the project" }),
		);
		expect(
			screen.queryByRole("button", { name: "Cancel this task" }),
		).toBeNull();
		expect((await sentCommand(page.s)).params).toEqual({
			command: { command: "task_integrate", body: { task_id: "FRK-1" } },
		});
		cleanup();

		// Added already: the day it was added.
		page = await opened({ ...CONTRACT, status: "accepted" }, [], {
			"task.history": {
				events: [
					...HISTORY,
					{
						...moved(11, "accepted", "human"),
						kind: "task.integrated",
						recorded_at: "2026-09-26T10:00:00Z",
						body: { commit: "abc", branch: "feature/FRK-1" },
					},
				],
			},
		});
		expect(
			await screen.findByText("Added on Saturday 26 September."),
		).toBeTruthy();
		expect(
			screen.queryByRole("button", { name: "Add to the project" }),
		).toBeNull();
		cleanup();

		// Escalated: cancelling resolves the escalation.
		page = await opened({ ...CONTRACT, status: "escalated" });
		fireEvent.click(
			await screen.findByRole("button", { name: "Cancel this task" }),
		);
		const asked = screen.getByRole("dialog");
		fireEvent.change(within(asked).getByLabelText(/Why/), {
			target: { value: "Too costly." },
		});
		fireEvent.click(
			within(asked).getByRole("button", { name: "Cancel this task" }),
		);
		expect((await sentCommand(page.s)).params).toEqual({
			command: {
				command: "escalation_resolve",
				body: { task_id: "FRK-1", to: "cancelled", message: "Too costly." },
			},
		});
		cleanup();

		// Cancelled: nothing to cancel.
		await opened({ ...CONTRACT, status: "cancelled" });
		await waitFor(() => screen.getByRole("heading", { level: 1 }));
		expect(
			screen.queryByRole("button", { name: "Cancel this task" }),
		).toBeNull();
	});
});
