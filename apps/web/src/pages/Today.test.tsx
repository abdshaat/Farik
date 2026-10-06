import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConnectionProvider } from "../app/connection.tsx";
import { en } from "../strings/en.ts";
import { type FakeSocket, socketsMade } from "../test/fake-socket.ts";
import { PLAN, SUMMARY, TITLE } from "../test/marketing.ts";
import { GOING_OUT, MARKUP, POST_ROW, todayWith } from "../test/posts.ts";
import {
	answerQuery,
	answerStatus,
	eventArrives,
	renderApp,
} from "../test/render-app.tsx";
import styles from "./PostGoingOut.module.css";
import { Today } from "./Today.tsx";

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

		// A refusal with no code is worded for the person, never shown as the daemon's text.
		fireEvent.click(screen.getByRole("button", { name: en.requestSend }));
		await waitFor(() => expect(s.calls("request.file")).toHaveLength(2));
		await s.fail(
			s.calls("request.file")[1] as never,
			-32603,
			"database is locked (SQLITE_BUSY)",
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseCommand,
		);

		fireEvent.change(box, {
			target: { value: "Add gift cards to the checkout page" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.requestSend }));
		await waitFor(() => expect(s.calls("request.file")).toHaveLength(3));
		await s.reply(s.calls("request.file")[2] as never, { task_id: "FRK-3" });
		// The request's own page asks for FRK-3.
		await waitFor(() =>
			expect(
				s.calls("query").find((q) => q.params.name === "contract.get")?.params
					.params,
			).toEqual({ task_id: "FRK-3" }),
		);
	});

	it("waits_for_the_connection_before_a_request_can_be_sent", async () => {
		// Today, drawn alone: the app's shell shows nothing until Farik has answered, so the page
		// is rendered here while the session check is still unanswered and there is no connection.
		let answerSession: (status: number) => void = () => {};
		const session = new Promise<Response>((resolve) => {
			answerSession = (status) => resolve(new Response(null, { status }));
		});
		vi.stubGlobal(
			"fetch",
			vi.fn(async (url: string) =>
				url === "/session" ? session : new Response(null, { status: 404 }),
			),
		);
		const { factory, sockets } = socketsMade();
		render(
			<ConnectionProvider socketFactory={factory}>
				<MemoryRouter initialEntries={["/"]}>
					<Today />
				</MemoryRouter>
			</ConnectionProvider>,
		);
		const box = await screen.findByRole("textbox", { name: en.requestLabel });
		fireEvent.change(box, { target: { value: "Add gift cards to checkout" } });
		const send = screen.getByRole("button", {
			name: en.requestSend,
		}) as HTMLButtonElement;
		expect(send.disabled).toBe(true);

		// Farik answers: the page connects, and the button works.
		await act(async () => answerSession(204));
		const socket = await waitFor(() => {
			const s = sockets[0];
			if (!s) throw new Error("no socket was opened");
			return s;
		});
		act(() => socket.emit("open", {}));
		await waitFor(() => expect(send.disabled).toBe(false));
		fireEvent.click(send);
		const filed = await waitFor(() => {
			const f = socket.calls("request.file")[0];
			if (!f) throw new Error("no request.file was sent");
			return f;
		});
		expect(filed.params).toEqual({ text: "Add gift cards to checkout" });
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

	it("today_skips_a_kind_it_does_not_know", async () => {
		const { container } = await today({
			waiting: [
				{
					task_id: "FRK-3",
					kind: "question",
					agent_id: "mira",
					title: "the launch post",
					line: "question line",
				},
				{
					task_id: "FRK-9",
					kind: "not_a_kind",
					agent_id: "mira",
					title: "something new",
					line: "a line of a later version",
				},
			],
		});

		const list = await screen.findByRole("list", { name: en.waitingList });
		const rows = within(list).getAllByRole("listitem");
		expect(rows).toHaveLength(1);
		expect(
			within(rows[0] as HTMLElement).getByText("Mira has a question"),
		).toBeTruthy();
		expect(screen.queryByText("something new")).toBeNull();
		expect(screen.queryByText("a line of a later version")).toBeNull();
		// The heading counts what it shows.
		expect(
			screen.getByRole("heading", { name: "Waiting on you (1)" }),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("today_lists_an_approval_and_opens_the_dialog", async () => {
		const { container } = await today({
			waiting: [
				{
					task_id: "FRK-14",
					kind: "tool_approval",
					agent_id: "theo",
					title: "Sold-out badge on the menu",
					line: "Theo wants to use github",
					approval: 31,
					server: "github",
					tool: "create_issue",
					input: '{"title":"Sold out"}',
				},
			],
		});
		const list = await screen.findByRole("list", { name: en.waitingList });
		const row = within(list).getByRole("listitem");
		expect(within(row).getByText("Theo wants to use github")).toBeTruthy();
		expect(
			within(row).getByText(
				"To create issue, for FRK-14 Sold-out badge on the menu. Theo waits until you decide.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(
			within(row).getByRole("button", { name: en.waitingReview }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Theo wants to use github",
		});
		expect(within(dialog).getByText("create_issue")).toBeTruthy();
		expect(
			within(dialog).getByText("github, which you added to Theo"),
		).toBeTruthy();
		expect(
			within(dialog)
				.getByRole("link", { name: "FRK-14 Sold-out badge on the menu" })
				.getAttribute("href"),
		).toBe("/tasks/FRK-14");
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
	/** The row `waiting.list` gives while Kai's marketing plan waits on the owner. */
	const PLAN_ROW = {
		task_id: "FRK-31",
		kind: "marketing_plan",
		agent_id: "kai",
		title: TITLE,
		line: `Kai proposes a marketing plan: ${TITLE}`,
		plan: "MP-3",
		summary: SUMMARY,
		total: "450.00",
		currency: "USD",
		starts_on: "2026-10-12",
		ends_on: "2026-11-22",
	};
	const WITH_KAI = {
		...TEAM,
		agents: [
			...TEAM.agents,
			agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
		],
	};

	it("today_shows_a_plan_to_approve_with_its_summary_and_budget", async () => {
		const { container } = await today({
			waiting: [PLAN_ROW],
			team: WITH_KAI,
		});
		const list = await screen.findByRole("list", { name: en.waitingList });
		const row = within(list).getByRole("listitem");
		expect(
			within(row).getByText(`Marketing plan to approve: ${TITLE}`),
		).toBeTruthy();
		// The whole summary is on the row, as text; the page has the rest.
		expect(within(row).getByText(SUMMARY)).toBeTruthy();
		// What the agent wrote is shown as typed: no markup of it became an element.
		expect(row.querySelector("b")).toBeNull();
		expect(within(row).getByText("$450.00")).toBeTruthy();
		expect(within(row).getByText("USD")).toBeTruthy();
		expect(
			within(row).getByText("Monday 12 October to Sunday 22 November"),
		).toBeTruthy();
		expect(within(row).getByRole("img").getAttribute("alt")).toBe("Kai");
		expect(
			screen.getByRole("heading", { name: "Waiting on you (1)" }),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("review_opens_the_plan_page", async () => {
		const { s } = await today({ waiting: [PLAN_ROW], team: WITH_KAI });
		const list = await screen.findByRole("list", { name: en.waitingList });
		const review = within(list).getByRole("link", { name: en.waitingReview });
		expect(review.getAttribute("href")).toBe("/marketing/plans/MP-3");
		fireEvent.click(review);

		// The page asks for that plan, and shows it.
		const asked = await waitFor(() => {
			const q = s
				.calls("query")
				.find((one) => one.params.name === "marketing_plan.get");
			if (!q) throw new Error("the plan was not asked for");
			return q;
		});
		expect(asked.params.params).toEqual({ plan: "MP-3" });
		await answerQuery(s, "team.get", { team: WITH_KAI });
		await answerQuery(s, "marketing_plan.get", PLAN);
		expect(
			await screen.findByRole("heading", {
				level: 1,
				name: TITLE,
			}),
		).toBeTruthy();
	});
});

/** The command the page sent, once it has sent `count`. */
const sent = (s: FakeSocket, count = 1) =>
	waitFor(() => {
		const c = s.calls("command")[count - 1];
		if (!c) throw new Error(`fewer than ${count} commands were sent`);
		return c;
	});

/** The rows of the list of posts going out. */
const goingOut = async () =>
	within(
		await screen.findByRole("list", { name: en.goingOutList }),
	).getAllByRole("listitem");

describe("today's posts", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("going_out_lists_posts_with_their_time_and_pictures", async () => {
		const { container, s } = await todayWith({ posts: GOING_OUT });

		expect(
			await screen.findByRole("heading", { name: "Going out (4)" }),
		).toBeTruthy();
		expect(
			screen.getByText(
				"Posts in your plan go out without asking you. Stop any of them before its time.",
			),
		).toBeTruthy();
		const rows = await goingOut();
		expect(rows).toHaveLength(4);
		const [x, instagram, later, last] = rows as [
			HTMLElement,
			HTMLElement,
			HTMLElement,
			HTMLElement,
		];

		// Soonest first, each with its network, its time, how soon, and why it may go out.
		expect(within(x).getByText("X, today at 10:00")).toBeTruthy();
		expect(within(x).getByText("in 45 minutes")).toBeTruthy();
		expect(x.textContent).toContain(
			"You allowed this. Buffer has it, and posts it at 10:00.",
		);
		expect(within(instagram).getByText("Ig")).toBeTruthy();
		expect(
			within(instagram).getByText("Instagram, today at 13:00"),
		).toBeTruthy();
		expect(within(instagram).getByText("in 3 hours 45 minutes")).toBeTruthy();
		expect(
			within(instagram).getByText("Thanksgiving pies are open for pre-order."),
		).toBeTruthy();
		expect(instagram.textContent).toContain(
			"Approved in your plan MP-3. Farik hands it to Buffer at 12:00.",
		);
		expect(
			within(instagram)
				.getByRole("link", { name: "MP-3" })
				.getAttribute("href"),
		).toBe("/marketing/plans/MP-3");
		expect(
			within(later).getByText("X, Wednesday 28 October at 08:30"),
		).toBeTruthy();
		expect(within(later).getByText("in 2 days")).toBeTruthy();
		expect(later.textContent).toContain(
			"Approved in your plan MP-3. Farik hands it to Buffer an hour before.",
		);
		expect(
			within(last).getByText("Instagram, Saturday 31 October at 09:00"),
		).toBeTruthy();
		expect(within(last).getByText("in 5 days")).toBeTruthy();
		for (const row of rows)
			expect(within(row).getByRole("button", { name: "Stop" })).toBeTruthy();

		// The daemon fetches each picture, since the browser may not load one from another site.
		const asked = await waitFor(() => {
			const frames = s.calls("social_post.media");
			if (frames.length === 0) throw new Error("no picture was asked for");
			return frames;
		});
		expect(asked.map((frame) => frame.params)).toEqual([
			{ post: 42, index: 0 },
		]);
		await s.reply(asked[0] as never, {
			media_type: "image/png",
			base64: "iVBORw==",
		});
		const picture = await within(instagram).findByRole("img", {
			name: "Picture 1 of this post",
		});
		expect(picture.getAttribute("src")).toBe("data:image/png;base64,iVBORw==");
		// A thumbnail of the mockup's size, not the picture as large as it is.
		expect(picture.getAttribute("width")).toBe("88");
		expect(picture.getAttribute("height")).toBe("88");
		await expectNoAxeViolations(container);
	});

	it("each_post_row_carries_its_agents_avatar", async () => {
		await todayWith({ posts: GOING_OUT, waiting: [POST_ROW] });

		// The agent who wrote a post is named by its picture, on each row that is about one.
		const going = await goingOut();
		const over = within(
			await screen.findByRole("list", { name: en.didNotGoOutList }),
		).getAllByRole("listitem");
		const asking = within(
			screen.getByRole("list", { name: en.waitingList }),
		).getAllByRole("listitem");
		expect([...going, ...over, ...asking]).toHaveLength(4 + 4 + 1);
		for (const row of [...going, ...over, ...asking])
			expect(within(row).getByRole("img", { name: "Kai" })).toBeTruthy();
	});

	it("what_farik_cannot_show_opens_in_a_new_tab", async () => {
		const { s } = await todayWith({ posts: GOING_OUT });
		const [, instagram, , last] = (await goingOut()) as [
			HTMLElement,
			HTMLElement,
			HTMLElement,
			HTMLElement,
		];

		// A clip is not fetched: it opens where it is, in a new tab that cannot reach this page.
		const clip = within(last).getByRole("link", { name: "Watch the clip" });
		expect(clip.getAttribute("href")).toBe(
			"https://cdn.example.com/cookies.mp4",
		);
		expect(clip.getAttribute("target")).toBe("_blank");
		expect(clip.getAttribute("rel")).toContain("noopener");
		expect(clip.getAttribute("rel")).toContain("noreferrer");
		expect(s.calls("social_post.media").map((frame) => frame.params)).toEqual([
			{ post: 42, index: 0 },
		]);

		// A picture the daemon would not fetch is opened the same way.
		const [frame] = s.calls("social_post.media");
		await s.fail(frame as never, -32002, "there is no picture to show");
		const picture = await within(instagram).findByRole("link", {
			name: "Open the picture",
		});
		expect(picture.getAttribute("href")).toBe(
			"https://cdn.example.com/pies.png",
		);
		expect(picture.getAttribute("target")).toBe("_blank");
		expect(picture.getAttribute("rel")).toContain("noopener");
		expect(picture.getAttribute("rel")).toContain("noreferrer");
		expect(
			within(instagram).queryByRole("img", { name: /^Picture/ }),
		).toBeNull();
	});

	it("never_opens_an_address_that_is_not_https", async () => {
		const [, , , clip] = GOING_OUT.posts;
		const posts = {
			posts: [
				{
					...clip,
					media: [
						{ url: "javascript:alert(1)", kind: "video" },
						{ url: "http://cdn.example.com/cookies.mp4", kind: "video" },
					],
				},
			],
		};
		await todayWith({ posts });

		const [row] = await goingOut();
		expect(row?.textContent).toContain("Halloween sugar cookies");
		expect(within(row as HTMLElement).queryAllByRole("link")).toHaveLength(1);
		expect(
			within(row as HTMLElement).queryByRole("link", {
				name: "Watch the clip",
			}),
		).toBeNull();
	});

	it("a_requested_post_offers_post_it_and_dont_post", async () => {
		const { container, s } = await todayWith({ waiting: [POST_ROW] });

		const list = await screen.findByRole("list", { name: en.waitingList });
		const row = within(list).getByRole("listitem");
		expect(
			within(row).getByText("Kai wants to post on Instagram"),
		).toBeTruthy();
		expect(
			within(row).getByText("It is not in your plan, so Kai asks first."),
		).toBeTruthy();
		expect(
			within(row).getByText("Instagram, Thursday 29 October at 18:00"),
		).toBeTruthy();
		// What the agent wrote is shown as typed, never as markup.
		expect(within(row).getByText(/Bake with us/).textContent).toContain(MARKUP);
		expect(container.querySelector("img[src='x']")).toBeNull();
		expect(
			within(row).getByText(
				"If you allow it, Farik sends it at its time, and it waits under Going out until then, with Stop.",
			),
		).toBeTruthy();
		expect(
			screen.getByRole("heading", { name: "Waiting on you (1)" }),
		).toBeTruthy();

		fireEvent.click(within(row).getByRole("button", { name: "Post it" }));
		const post = await sent(s);
		expect(post.params).toEqual({
			command: {
				command: "social_post_decide",
				body: { post: 45, decision: "post" },
			},
		});
		// A refusal is said in words, whatever the daemon's text is.
		await s.reply(post, {
			error: {
				kind: "refused",
				detail: "post_decided: post 45 is not waiting for your decision",
			},
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refusePostDecided,
		);

		fireEvent.click(
			within(row).getByRole("button", { name: "Don\u2019t post" }),
		);
		const decline = await sent(s, 2);
		expect(decline.params).toEqual({
			command: {
				command: "social_post_decide",
				body: { post: 45, decision: "dont_post" },
			},
		});
		await s.reply(decline, { said: "did not allow post 45", events: [9] });
		await expectNoAxeViolations(container);
	});

	it("a_post_that_did_not_go_out_says_why_as_text", async () => {
		const { container } = await todayWith({ posts: GOING_OUT });

		const heading = await screen.findByRole("heading", {
			name: "Did not go out, in the last 24 hours",
		});
		const section = heading.closest("section") as HTMLElement;
		const rows = within(
			within(section).getByRole("list", { name: en.didNotGoOutList }),
		).getAllByRole("listitem");
		expect(rows).toHaveLength(4);
		const [failed, missed, paused, undecided] = rows as [
			HTMLElement,
			HTMLElement,
			HTMLElement,
			HTMLElement,
		];

		expect(within(failed).getByText("Instagram, today at 08:00")).toBeTruthy();
		expect(within(failed).getByText("Failed")).toBeTruthy();
		// Buffer's words and the agent's are text: none of their markup became an element.
		expect(failed.textContent).toContain(
			`Buffer did not take it: \u201cThe image is too small ${MARKUP}\u201d`,
		);
		expect(failed.textContent).toContain(
			"Kai hears of this in its next session.",
		);
		expect(container.querySelector("img[src='x']")).toBeNull();
		// What was written is cut after two lines on the page and whole for a reader of it; a post
		// still going out is shown whole.
		const words = within(failed).getByText(/^Thanksgiving pies/);
		expect(words.textContent).toBe(`Thanksgiving pies ${MARKUP}`);
		expect(words.classList).toContain(styles.clamp);
		expect(failed.classList).toContain(styles.over);
		const [going] = await goingOut();
		expect(
			within(going as HTMLElement).getByText(/^We open at 10 today/).classList,
		).not.toContain(styles.clamp);
		expect(within(missed).getByText("X, yesterday at 18:00")).toBeTruthy();
		expect(within(missed).getByText("Missed")).toBeTruthy();
		expect(missed.textContent).toContain(
			"Farik could not hand it to Buffer before its time.",
		);
		expect(missed.textContent).toContain(
			"Kai hears of this in its next session.",
		);
		expect(paused.textContent).toContain(
			"The team was paused, so it was not sent.",
		);
		expect(undecided.textContent).toContain(
			"You had not decided by its time, so it was not sent.",
		);
		expect(
			within(undecided).getByText("Threads, yesterday at 10:00"),
		).toBeTruthy();
		// They are over: there is nothing to stop.
		expect(within(section).queryByRole("button", { name: "Stop" })).toBeNull();
		await expectNoAxeViolations(container);
	});
});
