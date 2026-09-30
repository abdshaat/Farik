import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { media } from "../test/media.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const WIDE = "(min-width: 1024px)";

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	status: "active",
});
const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		agent("sol", "Sol", "scrum_master", "scrum-master"),
		agent("theo", "Theo", "software_developer", "developer"),
		agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};

const task = (
	id: number,
	title: string,
	status: string,
	more: Record<string, unknown> = {},
) => ({
	task_id: `FRK-${id}`,
	kind: "task",
	title,
	status,
	risk: "low",
	triaged: true,
	locked: false,
	updated_seq: id,
	cost_usd: 0,
	iteration: 0,
	awaiting_integration: false,
	waiting_on_human: false,
	awaiting_approval: false,
	verifications: 0,
	rejections: 0,
	interventions: 0,
	...more,
});
const TASKS = [
	task(3, "Gift cards", "refining", { kind: "epic" }),
	task(4, "Gift card page", "ready", {
		parent: "FRK-3",
		assignee_id: "theo",
		sprint: "S2",
	}),
	task(5, "Email a receipt", "accepted", {
		parent: "FRK-3",
		assignee_id: "theo",
	}),
	task(6, "Holiday opening hours", "escalated", {
		assignee_id: "mira",
		awaiting_approval: true,
	}),
	task(7, "Launch post", "in_progress", {
		assignee_id: "kai",
		risk: "high",
		sprint: "S2",
	}),
	task(8, "Show sold-out items", "rejected", { assignee_id: "theo" }),
	task(9, "New checkout page", "verifying", { assignee_id: "theo" }),
	task(10, "An old idea", "cancelled"),
	task(11, "Price list", "blocked", { assignee_id: "theo", risk: "medium" }),
	task(12, "Opening sale", "escalated", { assignee_id: "kai" }),
	task(13, "Loyalty stamps", "ready"),
	task(14, "Opening hours sign", "ready"),
];
const WAITING = [
	{
		task_id: "FRK-6",
		kind: "approval",
		agent_id: "mira",
		title: "Holiday opening hours",
		line: "",
	},
	{
		task_id: "FRK-9",
		kind: "acceptance",
		agent_id: "theo",
		title: "New checkout page",
		line: "",
	},
	{
		task_id: "FRK-12",
		kind: "help",
		agent_id: "kai",
		title: "Opening sale",
		line: "",
	},
];
const ACTIVITY = {
	activity: [
		{
			agent_id: "kai",
			state: "working",
			line: "Building FRK-7",
			task_id: "FRK-7",
			session_id: "s-1",
			purpose: "implement",
		},
		{ agent_id: "theo", state: "idle", line: "Nothing to do yet" },
	],
};
const SPRINTS = {
	sprints: ["S1", "S2"].map((sprint_id) => ({
		sprint_id,
		status: "ended",
		started_at: "2026-09-20T09:00:00Z",
		started_by: "human",
		ended_at: "2026-09-21T09:00:00Z",
		budget_usd: null,
		spent_usd: 1,
		planned_by: "sol",
		task_count: 1,
		done_count: 1,
	})),
};

/** The board, with each query answered. */
async function board(
	sprint: unknown = { sprint_id: "S2", done: 1, total: 3 },
	tasks: object[] = TASKS,
	activity: object = ACTIVITY,
) {
	const { container, socket } = await renderApp("/board");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "tasks.list", { tasks });
	await answerQuery(s, "waiting.list", { waiting: WAITING });
	await answerQuery(s, "team.activity", activity);
	await answerQuery(s, "sprint.current", sprint);
	// Until the sprints are listed, the next sprint's number is not known: nothing is drawn.
	await act(async () => {});
	expect(screen.queryByRole("link", { name: "Loyalty stamps" })).toBeNull();
	await answerQuery(s, "sprints.list", SPRINTS);
	await screen.findByRole("link", { name: "Loyalty stamps" });
	return { container, s };
}

/** The ids of the tasks the board shows, in lane order. */
const shown = () =>
	screen
		.getAllByRole("link")
		.map((a) => a.getAttribute("href") ?? "")
		.filter((href) => /^\/tasks\/FRK-\d+$/.test(href))
		.map((href) => href.slice("/tasks/".length))
		.sort();

const chip = (name: string) => screen.getByRole("button", { name });

describe("board", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_each_row_with_its_mark", async () => {
		media.set(WIDE, true);
		const { container } = await board();
		const lane = (name: string) => screen.getByRole("region", { name });
		const row = (lane: HTMLElement, title: string) =>
			within(lane)
				.getByRole("link", { name: title })
				.closest("li") as HTMLElement;

		const review = row(lane(en.statusReview), "New checkout page");
		expect(within(review).getByRole("img").getAttribute("alt")).toBe("Theo");
		expect(
			within(review)
				.getByRole("link", { name: "New checkout page" })
				.getAttribute("href"),
		).toBe("/tasks/FRK-9");
		expect(within(review).getByText("FRK-9")).toBeTruthy();
		expect(within(review).getByText(en.statusWaiting)).toBeTruthy();

		const working = lane(en.statusInProgress);
		expect(
			within(row(working, "Launch post")).getByText(en.markBuilding),
		).toBeTruthy();
		expect(
			within(row(working, "Show sold-out items")).getByText(en.statusReworked),
		).toBeTruthy();
		const planning = lane(en.statusPlanning);
		expect(
			within(row(planning, "Holiday opening hours")).getByText(
				en.statusWaiting,
			),
		).toBeTruthy();
		// An epic is a row with its parts' count; a part names its epic.
		expect(
			within(row(planning, "Gift cards")).getByText("2 parts, 1 done"),
		).toBeTruthy();
		expect(
			within(row(lane(en.statusToDo), "Gift card page")).getByText(
				"Gift cards",
			),
		).toBeTruthy();
		const stuck = lane(en.statusStuck);
		expect(
			within(row(stuck, "Opening sale")).getByText(en.statusHelp),
		).toBeTruthy();
		expect(
			within(row(lane(en.statusDone), "Email a receipt")).getByText(
				en.markAccepted,
			),
		).toBeTruthy();
		// Cancelled work stays hidden until asked for.
		expect(screen.queryByRole("link", { name: "An old idea" })).toBeNull();
		await expectNoAxeViolations(container);

		// On a phone, the lanes are tabs with counts, and one lane shows.
		act(() => media.set(WIDE, false));
		const tab = screen.getByRole("button", {
			name: `${en.statusReview} 1`,
		});
		fireEvent.click(tab);
		expect(tab.getAttribute("aria-pressed")).toBe("true");
		expect(shown()).toEqual(["FRK-9"]);
		fireEvent.click(chip("Kai"));
		expect(screen.getByText(en.laneEmpty)).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("says_planning_while_the_designer_explores", async () => {
		media.set(WIDE, true);
		const exploring = {
			activity: [
				{ ...ACTIVITY.activity[0], line: "Planning FRK-7", purpose: "explore" },
			],
		};
		await board(undefined, TASKS, exploring);
		const row = within(
			screen.getByRole("region", { name: en.statusInProgress }),
		)
			.getByRole("link", { name: "Launch post" })
			.closest("li") as HTMLElement;
		expect(within(row).getByText(en.markPlanning)).toBeTruthy();
	});

	it("filters_the_board", async () => {
		media.set(WIDE, true);
		const { container } = await board();
		const all = [
			"FRK-11",
			"FRK-12",
			"FRK-13",
			"FRK-14",
			"FRK-3",
			"FRK-4",
			"FRK-5",
			"FRK-6",
			"FRK-7",
			"FRK-8",
			"FRK-9",
		];
		expect(shown()).toEqual(all);

		fireEvent.click(chip("Kai"));
		expect(chip("Kai").getAttribute("aria-pressed")).toBe("true");
		expect(shown()).toEqual(["FRK-12", "FRK-7"]);
		fireEvent.click(chip(en.filterEveryone));
		expect(shown()).toEqual(all);

		fireEvent.click(chip(en.filterWaiting));
		expect(shown()).toEqual(["FRK-12", "FRK-6", "FRK-9"]);
		fireEvent.click(chip(en.filterWaiting));

		fireEvent.click(chip("Gift cards"));
		expect(shown()).toEqual(["FRK-3", "FRK-4", "FRK-5"]);
		fireEvent.click(chip("Gift cards"));
		expect(shown()).toEqual(all);

		fireEvent.click(screen.getByText(en.filterMore));
		const sprint = screen.getByLabelText(en.filterSprint);
		fireEvent.change(sprint, { target: { value: "this" } });
		expect(shown()).toEqual(["FRK-4", "FRK-7"]);
		fireEvent.change(sprint, { target: { value: "none" } });
		expect(shown()).toEqual(
			all.filter((id) => id !== "FRK-4" && id !== "FRK-7"),
		);
		fireEvent.change(sprint, { target: { value: "all" } });

		const risk = screen.getByLabelText(en.filterRisk);
		fireEvent.change(risk, { target: { value: "high" } });
		expect(shown()).toEqual(["FRK-7"]);
		fireEvent.change(risk, { target: { value: "medium" } });
		expect(shown()).toEqual(["FRK-11"]);
		fireEvent.change(risk, { target: { value: "any" } });

		fireEvent.click(screen.getByLabelText(en.filterCancelled));
		expect(shown()).toEqual([...all, "FRK-10"].sort());
		expect(
			within(screen.getByRole("region", { name: en.statusDone })).getByText(
				en.statusCancelled,
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("starts_and_ends_a_sprint_from_the_board", async () => {
		media.set(WIDE, true);
		// A ready part of an epic is planned with its epic, not picked into a sprint.
		const first = await board(null, [
			...TASKS,
			task(15, "Gift card email", "ready", { parent: "FRK-3" }),
		]);
		expect(screen.getByText(en.sprintNone)).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: en.sprintStart }));
		const dialog = screen.getByRole("dialog", { name: "Start sprint 3" });
		// Sol, the Scrum Master, plans it, from the two ready tasks with no epic and no sprint.
		expect(
			within(dialog).getByText(/^Sol will plan it: Sol picks from the 2 ready/),
		).toBeTruthy();
		expect(
			(within(dialog).getByLabelText(en.sprintNoLimit) as HTMLInputElement)
				.checked,
		).toBe(true);
		await expectNoAxeViolations(first.container);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Start sprint 3" }),
		);
		const sent = await waitFor(() => {
			const frame = first.s.calls("command")[0];
			if (!frame) throw new Error("no command was sent");
			return frame;
		});
		expect(sent.params).toEqual({
			command: { command: "sprint_start", body: { budget_usd: null } },
		});
		// A refusal is said in words, and the dialog stays.
		await first.s.reply(sent, {
			error: { kind: "refused", detail: "sprint_open: a sprint is open" },
		});
		expect(await within(dialog).findByText(en.refuseOther)).toBeTruthy();

		// With a limit, the budget is sent in dollars.
		fireEvent.click(within(dialog).getByLabelText(en.sprintLimitAfter));
		expect(
			(within(dialog).getByLabelText(en.sprintLimitDollars) as HTMLInputElement)
				.value,
		).toBe("20.00");
		fireEvent.change(within(dialog).getByLabelText(en.sprintLimitDollars), {
			target: { value: "12.50" },
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Start sprint 3" }),
		);
		await waitFor(() =>
			expect(first.s.calls("command")[1]?.params).toEqual({
				command: { command: "sprint_start", body: { budget_usd: 12.5 } },
			}),
		);
		cleanup();

		const second = await board({ sprint_id: "S2", done: 1, total: 3 });
		// The sprint line opens the sprint's page.
		expect(
			screen
				.getByRole("link", { name: "Sprint 2 is running: 1 of 3 tasks done" })
				.getAttribute("href"),
		).toBe("/sprints/S2");
		fireEvent.click(screen.getByRole("button", { name: en.sprintEndEarly }));
		const end = screen.getByRole("dialog", { name: "End sprint 2 early?" });
		expect(
			within(end).getByText(
				"2 tasks are not finished. They leave the sprint and go back on the board exactly as they are. Nothing is lost, and work in progress keeps going. Sol will still run the review and the look back.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(second.container);
		fireEvent.click(within(end).getByRole("button", { name: "End sprint 2" }));
		await waitFor(() =>
			expect(second.s.calls("command")[0]?.params).toEqual({
				command: { command: "sprint_end", body: {} },
			}),
		);
		cleanup();

		// One unfinished task is said in the singular.
		await board({ sprint_id: "S2", done: 2, total: 3 });
		fireEvent.click(screen.getByRole("button", { name: en.sprintEndEarly }));
		expect(
			within(
				screen.getByRole("dialog", { name: "End sprint 2 early?" }),
			).getByText(
				"1 task is not finished. It leaves the sprint and goes back on the board exactly as it is. Nothing is lost, and work in progress keeps going. Sol will still run the review and the look back.",
			),
		).toBeTruthy();
	});
});
