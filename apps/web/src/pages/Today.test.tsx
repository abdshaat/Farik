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
import {
	answerQuery,
	answerStatus,
	eventArrives,
	renderApp,
} from "../test/render-app.tsx";

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
		agent("ada", "Ada", "architect", "architect"),
		agent("theo", "Theo", "software_developer", "developer"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};
const ACTIVITY = {
	activity: [
		{ agent_id: "mira", state: "working", line: "Writing the plan for FRK-2" },
		{
			agent_id: "ada",
			state: "resting",
			line: "Resting until 15:40 UTC",
			until: "2026-09-29T15:40:00Z",
		},
		{ agent_id: "theo", state: "idle", line: "Nothing to do yet" },
	],
};

/** Today, with the status, the team and each list answered. */
async function today(fields: {
	activity?: unknown;
	waiting?: unknown[];
	moved?: unknown[];
	sprint?: unknown;
	team?: unknown;
	backlog?: unknown;
}) {
	const { container, socket } = await renderApp("/");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: fields.team ?? TEAM });
	await answerQuery(s, "team.activity", fields.activity ?? ACTIVITY);
	await answerQuery(s, "waiting.list", { waiting: fields.waiting ?? [] });
	await answerQuery(s, "moved.since", { moved: fields.moved ?? [] });
	await answerQuery(s, "sprint.current", fields.sprint ?? null);
	if (fields.backlog) await answerQuery(s, "backlog.summary", fields.backlog);
	return { container, s };
}

describe("today", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_the_team_band", async () => {
		const { container } = await today({
			sprint: { sprint_id: "S2", done: 4, total: 7 },
		});
		const band = await screen.findByRole("list", { name: en.teamBand });
		const entries = within(band).getAllByRole("listitem");
		expect(entries).toHaveLength(3);
		const [mira, ada, theo] = entries as HTMLElement[];
		expect(
			within(mira as HTMLElement)
				.getByRole("img")
				.getAttribute("alt"),
		).toBe("Mira");
		expect(
			within(mira as HTMLElement).getByText("Mira", { selector: "strong" }),
		).toBeTruthy();
		expect(
			within(mira as HTMLElement).getByTitle("Product Manager"),
		).toBeTruthy();
		expect(
			within(mira as HTMLElement).getByText("Writing the plan for FRK-2"),
		).toBeTruthy();
		expect(
			within(ada as HTMLElement).getByText("Resting until 15:40 UTC"),
		).toBeTruthy();
		expect(within(theo as HTMLElement).getByTitle("Developer")).toBeTruthy();
		expect(
			screen.getByText("Sprint 2 is running: 4 of 7 tasks done"),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("sends_a_request_to_the_team", async () => {
		const { container, s } = await today({});
		const box = await screen.findByRole("textbox", {
			name: en.requestLabel,
		});
		expect(
			screen.getByText(
				"Mira reads every request and asks you if anything is unclear.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.change(box, { target: { value: "hi" } });
		fireEvent.click(screen.getByRole("button", { name: en.requestSend }));
		const refused = await waitFor(() => {
			const f = s.calls("request.file")[0];
			if (!f) throw new Error("no request.file was sent");
			return f;
		});
		expect(refused.params).toEqual({ text: "hi" });
		// The page words the refusal by its code, whatever the daemon's words are.
		await s.fail(refused, -32005, "write at least 20 characters", {
			errors: [
				{
					path: "/text",
					message: "write at least 20 characters",
					code: "too_short",
				},
			],
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.requestTooShort,
		);

		fireEvent.change(box, {
			target: { value: "Add gift cards to the checkout page" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.requestSend }));
		await waitFor(() => expect(s.calls("request.file")).toHaveLength(2));
		await s.reply(s.calls("request.file")[1] as never, { task_id: "FRK-3" });
		// The request's own page asks for FRK-3.
		await waitFor(() =>
			expect(
				s.calls("query").find((q) => q.params.name === "contract.get")?.params
					.params,
			).toEqual({ task_id: "FRK-3" }),
		);
	});

	it("lists_what_waits_on_you", async () => {
		const row = (
			task_id: string,
			kind: string,
			title: string,
			agent_id: string,
		) => ({
			task_id,
			kind,
			agent_id,
			title,
			line: `${kind} line`,
		});
		const { container, s } = await today({
			waiting: [
				row("FRK-1", "approval", "gift cards", "mira"),
				row("FRK-2", "acceptance", "the new checkout page", "theo"),
				row("FRK-3", "question", "the launch post", "mira"),
				row("FRK-4", "help", "the menu page", "theo"),
				row("FRK-5", "integration", "the photos", "theo"),
			],
		});
		// A key that works adds no row.
		await answerQuery(s, "account.status", {
			provider: "anthropic",
			kind: "subscription_token",
			source: "keychain",
		});
		expect(
			await screen.findByRole("heading", { name: "Waiting on you (5)" }),
		).toBeTruthy();
		expect(screen.queryByText(en.waitingKeyRefused)).toBeNull();
		const list = screen.getByRole("list", { name: en.waitingList });
		const rows = within(list).getAllByRole("listitem") as HTMLElement[];
		const expected = [
			[
				"approval",
				"Approve the plan for gift cards",
				"Review",
				"/tasks/FRK-1/plan",
			],
			[
				"acceptance",
				"Accept the new checkout page",
				"Review",
				"/tasks/FRK-2/accept",
			],
			["question", "Mira has a question", "Answer", "/tasks/FRK-3/questions"],
			["help", "Theo needs your help", "Help", "/tasks/FRK-4/help"],
			[
				"integration",
				"Add the photos to your project",
				"Add",
				"/tasks/FRK-5/accept",
			],
		] as const;
		expected.forEach(([kind, title, word, route], i) => {
			const one = rows[i] as HTMLElement;
			expect(within(one).getByText(title)).toBeTruthy();
			expect(within(one).getByText(`${kind} line`)).toBeTruthy();
			const link = within(one).getByRole("link", { name: word });
			expect(link.getAttribute("href")).toBe(route);
		});
		// The acceptance row says the checks passed, when every one did.
		await answerQuery(s, "task.checks", {
			checks: [1, 2, 3].map((n) => ({
				criterion_id: `c${n}`,
				text: `check ${n}`,
				passed: true,
			})),
		});
		expect(
			await within(rows[1] as HTMLElement).findByText(
				"All 3 of Farik’s checks passed.",
			),
		).toBeTruthy();
		expect(
			s
				.calls("query")
				.filter((q) => q.params.name === "task.checks")
				.map((q) => q.params.params),
		).toEqual([{ task_id: "FRK-2" }]);
		// Once one check fails, the row no longer says they passed.
		await eventArrives(s, 40);
		const checks = () =>
			s.calls("query").filter((q) => q.params.name === "task.checks");
		await waitFor(() => expect(checks()).toHaveLength(2));
		await s.reply(checks()[1] as never, {
			checks: [
				{ criterion_id: "c1", text: "check 1", passed: true },
				{ criterion_id: "c2", text: "check 2", passed: false },
			],
		});
		await waitFor(() =>
			expect(
				within(rows[1] as HTMLElement).queryByText(/checks passed/),
			).toBeNull(),
		);
		await expectNoAxeViolations(container);
	});

	it("links_the_waiting_rows_to_settings", async () => {
		const { container } = await today({
			waiting: [
				{
					task_id: "FRK-22",
					kind: "preview_missing",
					agent_id: "iris",
					title: "The Order again button",
					line: "iris needs to know how to open your app",
				},
				{
					task_id: "FRK-23",
					kind: "designer_needs_sandbox",
					agent_id: "iris",
					title: "A bigger basket",
					line: "The UI/UX Designer needs Docker's sandbox to open your app. Turn the sandbox on, or retire the Designer",
				},
			],
			team: {
				...TEAM,
				agents: [
					...TEAM.agents,
					agent("iris", "Iris", "ui_ux_designer", "extra-1"),
				],
			},
		});
		const list = await screen.findByRole("list", { name: en.waitingList });
		const [missing, sandbox] = within(list).getAllByRole(
			"listitem",
		) as HTMLElement[];
		expect(
			within(missing as HTMLElement).getByText(en.waitingPreviewMissing),
		).toBeTruthy();
		expect(
			within(missing as HTMLElement).getByText(
				"Iris needs it to look at your screens. Until then Iris takes no work, and nobody checks the screens Theo builds.",
			),
		).toBeTruthy();
		expect(
			within(missing as HTMLElement)
				.getByRole("link", { name: en.waitingOpenSettings })
				.getAttribute("href"),
		).toBe("/settings#preview");
		expect(
			within(sandbox as HTMLElement).getByText("Iris needs Docker’s sandbox"),
		).toBeTruthy();
		expect(
			within(sandbox as HTMLElement).getByText(en.waitingNeedsSandboxLine),
		).toBeTruthy();
		expect(
			within(sandbox as HTMLElement)
				.getByRole("link", { name: en.waitingOpenTeam })
				.getAttribute("href"),
		).toBe("/team");
		await expectNoAxeViolations(container);
	});

	it("links_the_browser_row_to_the_designers_page", async () => {
		const { container } = await today({
			waiting: [
				{
					task_id: "FRK-24",
					kind: "designer_needs_browser",
					agent_id: "iris",
					title: "A bigger basket",
					line: "Iris has Playwright off, so Farik gives Iris no work. Turn Playwright on for Iris on the Team page",
				},
			],
			team: {
				...TEAM,
				agents: [
					...TEAM.agents,
					agent("iris", "Iris", "ui_ux_designer", "extra-1"),
				],
			},
		});
		const list = await screen.findByRole("list", { name: en.waitingList });
		const [row] = within(list).getAllByRole("listitem") as HTMLElement[];
		expect(
			within(row as HTMLElement).getByText("Iris’s browser is off"),
		).toBeTruthy();
		expect(
			within(row as HTMLElement).getByText(
				"Iris opens your app with Playwright. Turn Playwright on for Iris on the Team page; until then Iris takes no work, and nobody checks the screens Theo builds.",
			),
		).toBeTruthy();
		expect(
			within(row as HTMLElement)
				.getByRole("link", { name: en.waitingOpenTeam })
				.getAttribute("href"),
		).toBe("/team/iris");
		await expectNoAxeViolations(container);
	});

	it("says_the_key_did_not_work_and_links_to_settings", async () => {
		const { container, s } = await today({});
		await answerQuery(s, "account.status", {
			provider: "anthropic",
			kind: "subscription_token",
			source: "keychain",
			key_refused: true,
		});
		expect(
			await screen.findByRole("heading", { name: "Waiting on you (1)" }),
		).toBeTruthy();
		const list = screen.getByRole("list", { name: en.waitingList });
		const [row] = within(list).getAllByRole("listitem") as HTMLElement[];
		expect(
			within(row as HTMLElement).getByText(
				"Your AI account’s key did not work",
			),
		).toBeTruthy();
		const link = within(row as HTMLElement).getByRole("link", {
			name: en.waitingKeyConnect,
		});
		expect(link.getAttribute("href")).toBe("/settings");
		await expectNoAxeViolations(container);
	});

	it("says_what_moved", async () => {
		const now = new Date().toISOString().replace(/\.\d+Z$/, "Z");
		const { container, s } = await today({
			moved: [
				{
					at: now,
					line: "Theo finished the checkout page and asked Ada to review it.",
				},
				{
					at: "2026-01-01T21:40:00Z",
					line: "The menu page's new photos were accepted and merged.",
				},
			],
		});
		const list = await screen.findByRole("list", { name: en.movedTitle });
		const time = within(list).getByText(now.slice(11, 16));
		expect(time.getAttribute("datetime")).toBe(now);
		// What moved on an earlier day says so, as the mockup's "Yday".
		const [, older] = within(list).getAllByRole("listitem");
		expect(
			within(older as HTMLElement).getByTitle("Yesterday").textContent,
		).toBe("Yday");
		expect(
			within(list).getByText(
				"Theo finished the checkout page and asked Ada to review it.",
			),
		).toBeTruthy();
		// Since yesterday: 24 hours back from now.
		const asked = s.calls("query").find((q) => q.params.name === "moved.since");
		const since = Date.parse(
			(asked?.params.params as { since: string } | undefined)?.since ?? "",
		);
		expect(Math.abs(Date.now() - 86_400_000 - since)).toBeLessThan(60_000);
		await expectNoAxeViolations(container);
	});

	it("counts_the_backlog_on_today", async () => {
		const band = async () =>
			(await screen.findByRole("list", { name: en.teamBand }))
				.parentElement as HTMLElement;
		const first = await today({ backlog: { plan_in_sprints: true, count: 2 } });
		const line = await within(await band()).findByText(
			/^2 pieces of work are ready and wait in the Backlog\./,
		);
		expect(line.textContent).toBe(
			"2 pieces of work are ready and wait in the Backlog. Start a sprint to begin them.",
		);
		expect(
			within(line)
				.getByRole("link", { name: en.todayBacklogStart })
				.getAttribute("href"),
		).toBe("/board?start=sprint");
		await expectNoAxeViolations(first.container);
		cleanup();

		await today({ backlog: { plan_in_sprints: true, count: 1 } });
		expect(
			(
				await within(await band()).findByText(
					/^1 piece of work is ready and waits in the Backlog\./,
				)
			).textContent,
		).toBe(
			"1 piece of work is ready and waits in the Backlog. Start a sprint to begin it.",
		);
		cleanup();

		// While a sprint runs, the late work waits for the next one.
		const second = await today({
			sprint: { sprint_id: "S3", done: 1, total: 5 },
			backlog: { plan_in_sprints: true, count: 1 },
		});
		const sprint = await screen.findByRole("link", {
			name: "Sprint 3 is running: 1 of 5 tasks done",
		});
		expect(sprint.getAttribute("href")).toBe("/sprints/S3");
		await waitFor(() =>
			expect(sprint.parentElement?.textContent).toBe(
				"Sprint 3 is running: 1 of 5 tasks done. 1 more waits in the Backlog for the next sprint.",
			),
		);
		await expectNoAxeViolations(second.container);
		cleanup();

		// Nothing waits, or the team does not plan in sprints: no line.
		for (const backlog of [
			{ plan_in_sprints: true, count: 0 },
			{ plan_in_sprints: false, count: 0 },
		]) {
			await today({ backlog });
			await act(async () => {});
			expect(within(await band()).queryByText(/Backlog/)).toBeNull();
			cleanup();
		}
	});
});
