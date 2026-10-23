import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expectNoAxeViolations } from "@catervas/ui/test";
import { act, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { media } from "../test/media.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
import styles from "./Board.module.css";

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
	task_id: `CTV-${id}`,
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
		parent: "CTV-3",
		assignee_id: "theo",
	}),
	task(5, "Email a receipt", "accepted", {
		parent: "CTV-3",
		assignee_id: "theo",
	}),
	task(6, "Holiday opening hours", "escalated", {
		assignee_id: "mira",
		awaiting_approval: true,
	}),
	task(7, "Launch post", "in_progress", {
		assignee_id: "kai",
		risk: "high",
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
		task_id: "CTV-6",
		kind: "approval",
		agent_id: "mira",
		title: "Holiday opening hours",
		line: "",
	},
	{
		task_id: "CTV-9",
		kind: "acceptance",
		agent_id: "theo",
		title: "New checkout page",
		line: "",
	},
	{
		task_id: "CTV-12",
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
			line: "Building CTV-7",
			task_id: "CTV-7",
			session_id: "s-1",
			purpose: "implement",
		},
		{ agent_id: "theo", state: "idle", line: "Nothing to do yet" },
	],
};
/** The board, with each query answered. */
async function board(
	tasks: object[] = TASKS,
	activity: object = ACTIVITY,
	team: object = TEAM,
	{
		path = "/board",
		waiting = WAITING,
	}: { path?: string; waiting?: object[] } = {},
) {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team });
	await answerQuery(s, "tasks.list", { tasks });
	await answerQuery(s, "waiting.list", { waiting });
	await answerQuery(s, "team.activity", activity);
	const last = (tasks.at(-1) as { title: string }).title;
	await screen.findByRole("link", { name: last });
	return { container, s };
}

/** The ids of the tasks the board shows, in lane order. */
const shown = () =>
	screen
		.getAllByRole("link")
		.map((a) => a.getAttribute("href") ?? "")
		.filter((href) => /^\/t\/shop\/tasks\/CTV-\d+$/.test(href))
		.map((href) => href.slice("/t/shop/tasks/".length))
		.sort();

const chip = (name: string) => screen.getByRole("button", { name });

describe("board", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_ready_work_to_do_with_no_sprint_anywhere", async () => {
		media.set(WIDE, true);
		const { s } = await board([
			task(1, "Ready one", "ready"),
			task(2, "Busy two", "in_progress", { assignee_id: "theo" }),
			task(3, "Stuck three", "escalated", { assignee_id: "theo" }),
		]);
		const todo = screen.getByRole("region", { name: en.statusToDo });
		expect(within(todo).getByRole("link", { name: "Ready one" })).toBeTruthy();
		expect(screen.queryByRole("region", { name: "Backlog" })).toBeNull();
		expect(screen.queryByRole("button", { name: /sprint/i })).toBeNull();
		fireEvent.click(screen.getByText(en.filterMore));
		expect(screen.queryByLabelText(/sprint/i)).toBeNull();
		expect(document.body.textContent).not.toMatch(/sprint/i);
		const asked = s.calls("query").map((q) => q.params.name);
		for (const name of [
			"sprint.current",
			"sprints.list",
			"sprint.get",
			"backlog.summary",
		])
			expect(asked).not.toContain(name);
	});

	it("shows_what_a_to_do_card_waits_on", async () => {
		media.set(WIDE, true);
		await board([
			task(1, "An old idea", "cancelled"),
			task(2, "Ready two", "ready"),
			task(3, "Waiting three", "ready", {
				waits_on: [{ task_id: "CTV-1", status: "cancelled" }],
			}),
		]);
		const todo = screen.getByRole("region", { name: en.statusToDo });
		const card = within(todo)
			.getByRole("link", { name: "Waiting three" })
			.closest("li") as HTMLElement;
		const meta = within(card).getByText("Waits on CTV-1 (cancelled)");
		expect(meta.className).toContain(styles.meta);
		const other = within(todo)
			.getByRole("link", { name: "Ready two" })
			.closest("li") as HTMLElement;
		expect(within(other).queryByText(/Waits on/)).toBeNull();
	});

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
		).toBe("/t/shop/tasks/CTV-9");
		expect(within(review).getByText("CTV-9").className).toContain(styles.meta);
		const heading = within(review).getByRole("link", {
			name: "New checkout page",
		});
		expect(heading.className).toContain(styles.title3);
		expect(heading.getAttribute("title")).toBe("New checkout page");
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
		const part = within(row(lane(en.statusToDo), "Gift card page")).getByText(
			"CTV-4 · Gift cards",
		);
		expect(part.className).toContain(styles.meta);
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
		expect(shown()).toEqual(["CTV-9"]);
		fireEvent.click(chip("Kai"));
		expect(screen.getByText(en.laneEmpty)).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("caps_done_and_shows_all", async () => {
		media.set(WIDE, true);
		const eight = [1, 2, 3, 4, 5, 6, 7, 8].map((n) =>
			task(n, `Done ${n}`, "accepted", { assignee_id: "theo" }),
		);
		const { container } = await board(eight, ACTIVITY, TEAM, {
			waiting: [
				{
					task_id: "CTV-2",
					kind: "integration",
					agent_id: "theo",
					title: "Done 2",
					line: "",
				},
			],
		});
		const titles = () =>
			within(screen.getByRole("region", { name: en.statusDone }))
				.getAllByRole("listitem")
				.map((li) => within(li).getByRole("link").textContent);
		expect(
			within(screen.getByRole("region", { name: en.statusDone })).getByRole(
				"heading",
				{ name: /8/ },
			),
		).toBeTruthy();
		expect(titles()).toEqual([
			"Done 2",
			"Done 4",
			"Done 5",
			"Done 6",
			"Done 7",
			"Done 8",
		]);
		const all = screen.getByRole("button", { name: "Show all 8" });
		expect(all.getAttribute("aria-expanded")).toBe("false");
		fireEvent.click(all);
		expect(titles()).toHaveLength(8);
		expect(titles()[0]).toBe("Done 1");
		expect(
			screen
				.getByRole("button", { name: en.boardShowFewer })
				.getAttribute("aria-expanded"),
		).toBe("true");
		await expectNoAxeViolations(container);
		fireEvent.click(screen.getByRole("button", { name: en.boardShowFewer }));

		// On a phone, the Done tab shows the same six and the same button.
		act(() => media.set(WIDE, false));
		fireEvent.click(screen.getByRole("button", { name: `${en.statusDone} 8` }));
		expect(titles()).toHaveLength(6);
		expect(screen.getByRole("button", { name: "Show all 8" })).toBeTruthy();
	});

	it("says_planning_while_the_designer_explores", async () => {
		media.set(WIDE, true);
		const exploring = {
			activity: [
				{ ...ACTIVITY.activity[0], line: "Planning CTV-7", purpose: "explore" },
			],
		};
		await board(TASKS, exploring);
		const row = within(
			screen.getByRole("region", { name: en.statusInProgress }),
		)
			.getByRole("link", { name: "Launch post" })
			.closest("li") as HTMLElement;
		expect(within(row).getByText(en.markPlanning)).toBeTruthy();
	});

	it("names_who_is_thinking_on_a_card", async () => {
		media.set(WIDE, true);
		const thinking = (purpose: string) => ({
			activity: [
				{
					agent_id: "mira",
					state: "working",
					line: "Writing the plan",
					task_id: "CTV-3",
					session_id: "s-2",
					purpose,
				},
				...ACTIVITY.activity,
				{
					agent_id: "theo",
					state: "working",
					line: "Building CTV-4",
					task_id: "CTV-4",
				},
			],
		});
		const { container } = await board(TASKS, thinking("refine"));
		const card = (title: string) =>
			screen.getByRole("link", { name: title }).closest("li") as HTMLElement;
		expect(
			within(card("Gift cards")).getByText(/Mira · Product Manager — thinking/),
		).toBeTruthy();
		expect(within(card("Gift cards")).getByText(en.markWriting)).toBeTruthy();
		expect(
			within(card("Launch post")).getByText(
				/Kai · Marketing Specialist — thinking/,
			),
		).toBeTruthy();
		expect(card("Gift card page").textContent).not.toContain("thinking");
		for (const dots of container.querySelectorAll(`.${styles.dots}`))
			expect(dots.getAttribute("aria-hidden")).toBe("true");
		const css = readFileSync(
			join(import.meta.dirname, "Board.module.css"),
			"utf8",
		).replace(/\s+/g, " ");
		expect(css).toMatch(/\.dots span \{[^}]*animation:/);
		expect(css).toMatch(
			/@media \(prefers-reduced-motion: reduce\) \{ \.dots span \{ animation: none;/,
		);
		await expectNoAxeViolations(container);
	});

	it("shows_planning_on_a_card_whose_agent_plans", async () => {
		media.set(WIDE, true);
		await board(TASKS, {
			activity: [
				{
					agent_id: "mira",
					state: "working",
					line: "Planning",
					task_id: "CTV-3",
					session_id: "s-2",
					purpose: "plan",
				},
				...ACTIVITY.activity,
			],
		});
		const card = screen
			.getByRole("link", { name: "Gift cards" })
			.closest("li") as HTMLElement;
		expect(within(card).getByText(en.markPlanning)).toBeTruthy();
	});

	it("marks_a_task_waiting_on_the_designer", async () => {
		media.set(WIDE, true);
		const withIris = {
			...TEAM,
			agents: [
				...TEAM.agents,
				agent("iris", "Iris", "ui_ux_designer", "extra-1"),
			],
		};
		const tasks = [
			...TASKS,
			task(22, "The Order again button", "verifying", {
				assignee_id: "theo",
				design_review_state: "waiting_on_designer",
			}),
			task(23, "A bigger basket", "verifying", { assignee_id: "theo" }),
			task(24, "A gift note", "verifying", {
				assignee_id: "theo",
				design_review_state: "designer_needs_browser",
			}),
		];
		const checking = {
			activity: [
				...ACTIVITY.activity,
				{
					agent_id: "iris",
					state: "working",
					line: "Checking CTV-23",
					task_id: "CTV-23",
					session_id: "s-2",
					purpose: "verify",
				},
			],
		};
		const { container, s } = await board(tasks, checking, withIris);
		const review = screen.getByRole("region", { name: en.statusReview });
		const row = (title: string) =>
			within(review)
				.getByRole("link", { name: title })
				.closest("li") as HTMLElement;
		// The Designer's own review session is checking the screens.
		expect(
			within(row("A bigger basket")).getByText(en.designChecking),
		).toBeTruthy();
		// A card with nothing else to say reads its design review's state from the list itself.
		expect(
			within(row("The Order again button")).getByText(en.designOnDesigner),
		).toBeTruthy();
		expect(
			within(row("A gift note")).getByText(en.designNoBrowser),
		).toBeTruthy();
		expect(
			within(row("A gift note")).queryByText(en.designOnDesigner),
		).toBeNull();
		expect(
			s.calls("query").filter((q) => q.params.name === "task.get"),
		).toEqual([]);
		await expectNoAxeViolations(container);
	});

	it("filters_the_board", async () => {
		media.set(WIDE, true);
		const { container } = await board();
		const all = [
			"CTV-11",
			"CTV-12",
			"CTV-13",
			"CTV-14",
			"CTV-3",
			"CTV-4",
			"CTV-5",
			"CTV-6",
			"CTV-7",
			"CTV-8",
			"CTV-9",
		];
		expect(shown()).toEqual(all);

		fireEvent.click(chip("Kai"));
		expect(chip("Kai").getAttribute("aria-pressed")).toBe("true");
		expect(shown()).toEqual(["CTV-12", "CTV-7"]);
		fireEvent.click(chip(en.filterEveryone));
		expect(shown()).toEqual(all);

		fireEvent.click(chip(en.filterWaiting));
		expect(shown()).toEqual(["CTV-12", "CTV-6", "CTV-9"]);
		fireEvent.click(chip(en.filterWaiting));

		fireEvent.click(chip("Gift cards"));
		expect(shown()).toEqual(["CTV-3", "CTV-4", "CTV-5"]);
		fireEvent.click(chip("Gift cards"));
		expect(shown()).toEqual(all);

		fireEvent.click(screen.getByText(en.filterMore));
		const risk = screen.getByLabelText(en.filterRisk);
		fireEvent.change(risk, { target: { value: "high" } });
		expect(shown()).toEqual(["CTV-7"]);
		fireEvent.change(risk, { target: { value: "medium" } });
		expect(shown()).toEqual(["CTV-11"]);
		fireEvent.change(risk, { target: { value: "any" } });

		fireEvent.click(screen.getByLabelText(en.filterCancelled));
		expect(shown()).toEqual([...all, "CTV-10"].sort());
		expect(
			within(screen.getByRole("region", { name: en.statusDone })).getByText(
				en.statusCancelled,
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
