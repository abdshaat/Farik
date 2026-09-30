import { expectNoAxeViolations } from "@farik/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import {
	COMPLETION,
	event,
	HISTORY,
	openedGate,
	REVIEW,
	sentCommand,
	TASK,
} from "../test/gate.ts";
import { TEAM } from "../test/plan.ts";

const PAGE = [
	"team.get",
	"task.get",
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
		"task.get": { task: {}, design_plan: null },
		...overrides,
	});

describe("task detail", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
		vi.useRealTimers();
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
		// The tabs' keys: Home and End go to the first and the last.
		fireEvent.keyDown(tabs[0] as HTMLElement, { key: "End" });
		expect(screen.getByRole("tab", { selected: true }).textContent).toBe(
			"Notes",
		);
		expect(document.activeElement?.textContent).toBe("Notes");
		fireEvent.keyDown(tabs[4] as HTMLElement, { key: "Home" });
		expect(screen.getByRole("tab", { selected: true }).textContent).toBe(
			"Summary and checks",
		);

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
		expect(
			within(panel()).getByText(
				"Medium. The team may still accept it without you.",
			),
		).toBeTruthy();
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
		cleanup();

		// Every kind the log records against a task has its own words.
		const at = "2026-09-25T10:00:00Z";
		const KINDS: [string, object, string?][] = [
			["contract.locked", { locked_by: "human" }],
			["contract.unlocked", { unlocked_by: "mira" }],
			["question.asked", { question: "Which amounts?", asked_by: "mira" }],
			[
				"question.answered",
				{ question_id: 1, answer: "Three.", answered_by: "human" },
			],
			[
				"request.triaged",
				{ size: "small", reason: "One page.", triaged_by: "mira" },
			],
			["budget.exhausted", { scope: "task", consequence: "escalated" }],
			[
				"transition.refused",
				{ from: "ready", to: "in_progress", actor: "governor" },
			],
			["escalation.aged", { raised_seq: 10, hours: 24 }],
			["pull_request.opened", { url: "u", number: 3, branch: "feature/FRK-1" }],
			[
				"sprint.planned",
				{ sprint_id: "S2", task_ids: ["FRK-1"], planned_by: "mira" },
			],
			["drift.detected", { drift: "lock_mismatch", detail: "d" }],
			[
				"message.posted",
				{ author: "theo", kind: "reply", text: "On it.", mentions: [] },
			],
			["tool.denied", { tool: "Bash", reason: "not allowed" }, "theo"],
			[
				"task.transitioned",
				{ from: "verifying", to: "rejected", requested_by: "ada" },
			],
		];
		await opened(CONTRACT, [], {
			"task.history": {
				events: KINDS.map(([kind, body, by], i) =>
					event(20 + i, kind, body, at, by),
				),
			},
		});
		await screen.findByRole("heading", { level: 1, name: "Gift cards" });
		fireEvent.click(screen.getByRole("tab", { name: "History" }));
		const told = within(panel())
			.getAllByRole("listitem")
			.map((line) => line.querySelector("span")?.textContent);
		expect(told).toHaveLength(KINDS.length);
		expect(told).not.toContain(en.toldOther);
		expect(told).toContain("You locked the plan.");
		expect(told).toContain("Farik stopped a step Theo tried.");
		expect(told).toContain("Ada sent it back.");
		expect(new Set(told).size).toBe(KINDS.length);
	});

	it("shows_the_plan_waiting_for_the_product_manager", async () => {
		// The plan was written the day the page is read, so the page says "today".
		vi.useFakeTimers({ toFake: ["Date"] });
		vi.setSystemTime(new Date("2026-09-30T12:00:00Z"));
		const PLAN =
			"The menu is hard to use on a phone. I will make the prices easy to see.\n\nWhat I will leave alone: the order page.";
		const proposed = event(
			11,
			"design_plan.proposed",
			{ plan: PLAN },
			"2026-09-30T09:52:00Z",
			"iris",
		);
		const decided = (kind: string, reason: string, seq = 12) =>
			event(seq, kind, { reason }, "2026-09-30T10:14:00Z", "mira");
		const open = (
			designPlan: object,
			extra: object[] = [],
			agents: object[] = TEAM.agents,
		) =>
			opened(
				{
					...CONTRACT,
					status: "in_progress",
					assignee: "iris",
					assignee_role: "ui_ux_designer",
				},
				[],
				{
					"team.get": {
						team: {
							...TEAM,
							agents: [
								...agents,
								{
									id: "iris",
									display_name: "Iris",
									role: "ui_ux_designer",
									avatar: "extra-1",
									status: "active",
								},
							],
						},
					},
					"task.history": { events: [...HISTORY, proposed, ...extra] },
					"task.get": { task: {}, design_plan: designPlan },
				},
			);
		const panel = () => screen.getByRole("tabpanel");
		const toPlan = async () => {
			fireEvent.click(await screen.findByRole("tab", { name: "The plan" }));
			return panel();
		};

		// Proposed: the Product Manager decides; the person need do nothing.
		const first = await open({ plan: PLAN, state: "proposed" });
		const heading = await screen.findByRole("heading", {
			level: 1,
			name: "Gift cards",
		});
		expect(
			within(heading.parentElement as HTMLElement).getByText(
				"Waiting for Mira to approve the plan",
			),
		).toBeTruthy();
		// The lead counts the Designer's plans, in place of the tries.
		expect(
			screen.getByText(
				"FRK-1 in sprint 2. Iris is doing it, and Ada reviews it. First plan.",
			),
		).toBeTruthy();
		let plan = await toPlan();
		expect(
			within(plan).getByText(
				"Iris looked at your app and wrote this. Iris changes nothing until Mira approves it.",
			),
		).toBeTruthy();
		expect(
			within(plan).getByText(
				"Mira checks that the plan does what the task asks, and nothing more. You do not need to do anything.",
			),
		).toBeTruthy();
		expect(
			within(plan).getByText("Iris wrote this plan today at 09:52"),
		).toBeTruthy();
		expect(
			within(plan).getByText(
				"The menu is hard to use on a phone. I will make the prices easy to see.",
			),
		).toBeTruthy();
		expect(
			within(plan).getByText("What I will leave alone: the order page."),
		).toBeTruthy();
		// The task's own plan is still there, under the Designer's.
		expect(within(plan).getByText("Gift cards bought online")).toBeTruthy();
		await expectNoAxeViolations(first.container);
		// The history says it in words, and a Designer's work is on a feature branch.
		fireEvent.click(screen.getByRole("tab", { name: "History" }));
		expect(within(panel()).getByText("Iris wrote a plan.")).toBeTruthy();
		fireEvent.click(screen.getByRole("tab", { name: "Code changes" }));
		expect(
			within(panel()).getByText(/on the branch feature\/FRK-1\./),
		).toBeTruthy();
		cleanup();

		// Approved: being built, with the Product Manager's reason.
		await open(
			{
				plan: PLAN,
				state: "approved",
				reason: "It keeps to the task. Go ahead.",
			},
			[decided("design_plan.approved", "It keeps to the task. Go ahead.")],
		);
		expect(await screen.findByText(en.designBeingBuilt)).toBeTruthy();
		plan = await toPlan();
		expect(
			within(plan).getByText("Mira approved the plan today at 10:14"),
		).toBeTruthy();
		expect(
			within(plan).getByText(
				"Iris is changing the page now, as the plan says.",
			),
		).toBeTruthy();
		expect(
			within(plan).getByText("“It keeps to the task. Go ahead.”"),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("tab", { name: "History" }));
		expect(
			within(panel()).getByText("Mira approved Iris’s plan."),
		).toBeTruthy();
		cleanup();

		// Returned: its reason, and how many more may be sent back.
		const returned = await open(
			{
				plan: PLAN,
				state: "returned",
				reason: "Keep the descriptions whole.",
			},
			[decided("design_plan.returned", "Keep the descriptions whole.")],
		);
		expect(await screen.findByText(en.designSentBack)).toBeTruthy();
		plan = await toPlan();
		expect(
			within(plan).getByText("Mira sent the plan back today at 10:14"),
		).toBeTruthy();
		expect(
			within(plan).getByText(
				"Iris is looking at the page again and will write a new plan.",
			),
		).toBeTruthy();
		expect(
			within(plan).getByText("“Keep the descriptions whole.”"),
		).toBeTruthy();
		expect(
			within(plan).getByText(
				"Plans sent back: 1 of 3. If a third is sent back, Farik stops the task and asks you.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(returned.container);
		fireEvent.click(screen.getByRole("tab", { name: "History" }));
		expect(
			within(panel()).getByText("Mira sent Iris’s plan back."),
		).toBeTruthy();
		cleanup();

		// A second plan, on a later day, with two paragraphs alike; and a paused Mira beside
		// an active Product Manager, who is the one that decides.
		vi.setSystemTime(new Date("2026-10-02T12:00:00Z"));
		const errors = vi.spyOn(console, "error").mockImplementation(() => {});
		const twice = "Same words.\n\nSame words.";
		await open(
			{ plan: twice, state: "proposed" },
			[
				decided("design_plan.returned", "Keep the descriptions whole."),
				event(
					13,
					"design_plan.proposed",
					{ plan: twice },
					"2026-10-01T09:00:00Z",
					"iris",
				),
			],
			[
				{ ...(TEAM.agents[0] as object), status: "paused" },
				...TEAM.agents.slice(1),
				{
					id: "pat",
					display_name: "Pat",
					role: "product_manager",
					avatar: "extra-3",
					status: "active",
				},
			],
		);
		expect(
			await screen.findByText(
				"FRK-1 in sprint 2. Iris is doing it, and Ada reviews it. Second plan.",
			),
		).toBeTruthy();
		plan = await toPlan();
		expect(
			within(plan).getByText("Waiting for Pat to approve the plan"),
		).toBeTruthy();
		expect(
			within(plan).getByText(
				"Iris wrote this plan on Thursday 1 October at 09:00",
			),
		).toBeTruthy();
		expect(within(plan).getAllByText("Same words.")).toHaveLength(2);
		expect(
			errors.mock.calls.some((call) => String(call[0]).includes("same key")),
		).toBe(false);
		errors.mockRestore();
		vi.useRealTimers();
	});

	it("says_what_each_risk_means", async () => {
		for (const [risk, words] of [
			["low", en.riskLowWhy],
			["high", en.riskHighWhy],
		] as const) {
			await opened({ ...CONTRACT, risk });
			await screen.findByRole("heading", { level: 1, name: "Gift cards" });
			fireEvent.click(screen.getByRole("tab", { name: "The plan" }));
			expect(
				within(screen.getByRole("tabpanel")).getByText(words),
			).toBeTruthy();
			cleanup();
		}
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
		// In review, with someone working on another task and a call for help on this one: no
		// add, no stop; cancel asks for a reason.
		let page = await opened(CONTRACT, [{ ...integration, kind: "help" }], {
			"team.activity": {
				activity: [
					{
						agent_id: "theo",
						state: "working",
						line: "Theo is building Receipts",
						task_id: "FRK-9",
						session_id: "s-9",
						purpose: "implement",
					},
				],
			},
		});
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
			target: { value: "   " },
		});
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

		// Accepted and being added by Farik on its own, or through a pull request.
		await opened({ ...CONTRACT, status: "accepted" });
		expect(
			await screen.findByText(
				"Accepted. Farik adds it to your project on its own.",
			),
		).toBeTruthy();
		cleanup();
		await opened({ ...CONTRACT, status: "accepted" }, [], {
			"team.get": {
				team: { ...TEAM, policy: { integration: "pull_request" } },
			},
		});
		expect(
			await screen.findByText(
				"Accepted. Farik opens a pull request for it, for you to merge.",
			),
		).toBeTruthy();
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
