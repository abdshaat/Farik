import { expectNoAxeViolations } from "@catervas/ui/test";
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
import {
	BUDGET_ROW,
	PLAN_BUDGET_ROW,
	REFUSED,
	RUNNING_PLAN,
	RUNNING_ROW,
	UNREAD_ROW,
} from "../test/ads.ts";
import { type FakeSocket, socketsMade } from "../test/fake-socket.ts";
import { CARD, docs as folderDocs } from "../test/folderDocs.ts";
import { sentCommand } from "../test/gate.ts";
import { KIT_ADS, KIT_LINEAR, KIT_NOTION } from "../test/kits.ts";
import {
	TEAM as MARKETING_TEAM,
	PLAN,
	SUMMARY,
	TITLE,
} from "../test/marketing.ts";
import {
	COMPARISON,
	LONG_ORDER_ROW,
	ORDER_ROW,
	RENEWALS,
	renewal,
	SCRIPT_ORDER_ROW,
	todayWithOrders,
	WRITTEN_ROW,
} from "../test/orders.ts";
import { picturesOpenTheirAgents } from "../test/pictures.ts";
import {
	DATA_ROW,
	ODD_ADDRESS_ROW,
	PAID_ROW,
	MARKUP as PIPELINE_MARKUP,
	TEAM as PIPELINE_TEAM,
	SCRIPT_PIPELINE_ROW,
	UNDECIDED_ROW,
} from "../test/pipelines.ts";
import { GOING_OUT, MARKUP, POST_ROW, todayWith } from "../test/posts.ts";
import { APPROVED, INTERVIEWING } from "../test/productPlan.ts";
import {
	answerQuery,
	answerStatus,
	eventArrives,
	renderApp,
} from "../test/render-app.tsx";
import { bodyOf, refusedBy } from "../test/schema.ts";
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
		{ agent_id: "mira", state: "working", line: "Writing the plan for CTV-2" },
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
	team?: unknown;
	docs?: unknown;
	plan?: unknown;
	catervafication?: unknown;
	kits?: unknown[];
	hiddenSuggestions?: unknown[];
	requestedConnections?: unknown[];
}) {
	const { container, socket } = await renderApp("/");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", {
		team: fields.team ?? TEAM,
		...(fields.kits && { kits: fields.kits }),
		...(fields.hiddenSuggestions && {
			hidden_suggestions: fields.hiddenSuggestions,
		}),
		...(fields.requestedConnections && {
			requested_connections: fields.requestedConnections,
		}),
	});
	await answerQuery(s, "team.activity", fields.activity ?? ACTIVITY);
	await answerQuery(s, "waiting.list", { waiting: fields.waiting ?? [] });
	await answerQuery(s, "moved.since", { moved: fields.moved ?? [] });
	if (fields.docs)
		await answerQuery(s, "folder_doc_proposals.list", fields.docs);
	if (fields.plan) await answerQuery(s, "product_plan.state", fields.plan);
	if (fields.catervafication)
		await answerQuery(s, "catervafication.get", fields.catervafication);
	return { container, s };
}

describe("today", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_the_team_band", async () => {
		const { container } = await today({});
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
			within(mira as HTMLElement).getByText("Writing the plan for CTV-2"),
		).toBeTruthy();
		expect(
			within(ada as HTMLElement).getByText("Resting until 15:40 UTC"),
		).toBeTruthy();
		expect(within(theo as HTMLElement).getByTitle("Developer")).toBeTruthy();
		picturesOpenTheirAgents(band, TEAM.agents);
		await expectNoAxeViolations(container);
	});

	it("opens_an_agents_page_from_its_picture_in_the_channel_preview", async () => {
		const { s } = await today({});
		await answerQuery(s, "channel.messages", {
			messages: [
				{
					seq: 1,
					at: "2026-09-28T09:02:00Z",
					author: "ada",
					kind: "chat",
					text: "The review is done.",
					mentions: [],
					thread: null,
					in_reply_to: null,
				},
			],
		});
		const preview = await screen.findByRole("region", {
			name: en.channelPreview,
		});
		await within(preview).findByText("The review is done.");
		picturesOpenTheirAgents(preview, TEAM.agents);
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
		await s.reply(s.calls("request.file")[2] as never, { task_id: "CTV-3" });
		// The request's own page asks for CTV-3.
		await waitFor(() =>
			expect(
				s.calls("query").find((q) => q.params.name === "contract.get")?.params
					.params,
			).toEqual({ task_id: "CTV-3" }),
		);
	});

	it("waits_for_the_connection_before_a_request_can_be_sent", async () => {
		// Today, drawn alone: the app's shell shows nothing until Catervas has answered, so the page
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

		// Catervas answers: the page connects, and the button works.
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

	it("adds_a_folder_change_from_its_row", async () => {
		const { s } = await today({
			waiting: [
				{
					kind: "integration",
					folder_change: 1,
					agent_id: "mira",
					title: "Roadmap and Product description from review 4",
					line: "You approved it; it waits for you to add it",
				},
				{
					task_id: "CTV-5",
					kind: "integration",
					agent_id: "theo",
					title: "the photos",
					line: "Accepted, waiting for you to add it",
				},
			],
		});
		const list = await screen.findByRole("list", { name: en.waitingList });
		const [folder, task] = within(list).getAllByRole(
			"listitem",
		) as HTMLElement[];
		expect(
			within(folder as HTMLElement).getByText(
				"Add Roadmap and Product description from review 4 to your project",
			),
		).toBeTruthy();
		expect(
			within(folder as HTMLElement).getByText(
				"You approved it; it waits for you to add it",
			),
		).toBeTruthy();
		// No task's page: the only link is the agent's picture.
		expect(
			within(folder as HTMLElement)
				.queryAllByRole("link")
				.map((link) => link.getAttribute("href")),
		).toEqual(["/team/mira"]);
		const add = within(folder as HTMLElement).getByRole("button", {
			name: "Add",
		});
		// The button is described by the row's title, as a task's link is.
		const describedBy = add.getAttribute("aria-describedby");
		expect(describedBy).toBeTruthy();
		expect(document.getElementById(describedBy as string)?.textContent).toBe(
			"Add Roadmap and Product description from review 4 to your project",
		);
		fireEvent.click(add);
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: { command: "folder_change_integrate", body: { change: 1 } },
		});
		expect(refusedBy("folderChangeIntegrateBody", bodyOf(sent))).toEqual([]);
		// A task's row still goes to its page.
		expect(
			within(task as HTMLElement)
				.getByRole("link", { name: "Add" })
				.getAttribute("href"),
		).toBe("/tasks/CTV-5/accept");
	});

	it("keeps_a_quiet_card_until_it_starts", async () => {
		const question = {
			task_id: "CTV-3",
			kind: "question",
			agent_id: "mira",
			title: "the launch post",
			line: "question line",
		};
		const { s, container } = await today({
			waiting: [question],
			catervafication: { offered: true, roles: ["product_manager"] },
		});
		const title = await screen.findByText(
			"Your team hasn\u2019t documented this project yet",
		);
		const start = screen.getByRole("button", { name: "Start" });
		expect(start.getAttribute("aria-describedby")).toBe(title.id);
		expect(
			screen.getByText(
				"A first piece of work where your team reads your project, writes down what it learns, looks for bugs and starts your marketing.",
			),
		).toBeTruthy();
		// It follows the request box, and is no "Waiting on you" item.
		const request = screen.getByLabelText(en.requestLabel);
		expect(
			request.compareDocumentPosition(title) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		expect(
			await screen.findByRole("heading", { name: "Waiting on you (1)" }),
		).toBeTruthy();
		const waitingList = screen.getByRole("list", { name: en.waitingList });
		expect(waitingList.contains(title)).toBe(false);
		await expectNoAxeViolations(container);

		fireEvent.click(start);
		const command = await sent(s);
		expect(command.params).toEqual({
			command: { command: "catervafication_start", body: {} },
		});
		expect(refusedBy("emptyBody", bodyOf(command))).toEqual([]);
		const asked = () =>
			s.calls("query").filter((f) => f.params.name === "catervafication.get");
		expect(asked()).toHaveLength(1);
		// A refusal is said under the card, which stays.
		await s.reply(command, {
			error: {
				kind: "refused",
				detail: "catervafication_started: the team is already documenting it",
			},
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			"Your team is already documenting this project.",
		);
		expect(asked()).toHaveLength(1);
		fireEvent.click(start);
		const again = await sent(s, 2);
		await s.reply(again, { said: "started", events: [9] });
		await waitFor(() => expect(asked()).toHaveLength(2));
		await answerQuery(s, "catervafication.get", { offered: false, roles: [] });
		await waitFor(() =>
			expect(screen.queryByText(title.textContent ?? "")).toBeNull(),
		);
		expect(screen.queryByRole("button", { name: "Start" })).toBeNull();
	});

	it("counts_and_lists_the_document_changes", async () => {
		const question = {
			task_id: "CTV-3",
			kind: "question",
			agent_id: "mira",
			title: "the launch post",
			line: "question line",
		};
		const { s } = await today({
			waiting: [question],
			docs: folderDocs([CARD]),
		});
		expect(
			await screen.findByRole("heading", { name: "Waiting on you (2)" }),
		).toBeTruthy();
		const list = screen.getByRole("list", { name: en.waitingList });
		const items = within(list).getAllByRole("listitem") as HTMLElement[];
		const region = await screen.findByRole("region", {
			name: en.docChangesTitle,
		});
		picturesOpenTheirAgents(region, TEAM.agents);
		const asked = items.findIndex((one) => one.contains(region));
		expect(asked).toBeGreaterThan(
			items.findIndex((one) => one.textContent?.includes("question line")),
		);
		expect(
			s
				.calls("query")
				.some((q) => q.params.name === "folder_doc_proposals.list"),
		).toBe(true);
	});

	it("counts_no_card_when_none_waits_and_lists_one_alone", async () => {
		cleanup();
		await today({
			waiting: [
				{
					task_id: "CTV-3",
					kind: "question",
					agent_id: "mira",
					title: "the launch post",
					line: "question line",
				},
			],
			docs: folderDocs([]),
		});
		expect(
			await screen.findByRole("heading", { name: "Waiting on you (1)" }),
		).toBeTruthy();
		cleanup();

		await today({ waiting: [], docs: folderDocs([CARD]) });
		expect(
			await screen.findByRole("heading", { name: "Waiting on you (1)" }),
		).toBeTruthy();
		expect(
			await screen.findByRole("region", { name: en.docChangesTitle }),
		).toBeTruthy();
		expect(screen.queryByText(en.waitingNone)).toBeNull();
	});

	it("says_the_team_waits_for_the_plan", async () => {
		await today({ plan: INTERVIEWING });
		const line = await screen.findByRole("link", {
			name: "Little Oak Cakes: your team waits for your product plan",
		});
		// The mockup draws it as a link: to the Product Manager's chat.
		expect(line.getAttribute("href")).toBe("/channel/mira");
		expect(line.closest("p")?.textContent).toBe(line.textContent);
		cleanup();

		await today({ plan: APPROVED });
		await screen.findByRole("list", { name: en.teamBand });
		expect(
			screen.queryByText(/your team waits for your product plan/),
		).toBeNull();
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
				row("CTV-1", "approval", "gift cards", "mira"),
				row("CTV-2", "acceptance", "the new checkout page", "theo"),
				row("CTV-3", "question", "the launch post", "mira"),
				row("CTV-4", "help", "the menu page", "theo"),
				row("CTV-5", "integration", "the photos", "theo"),
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
				"/tasks/CTV-1/plan",
			],
			[
				"acceptance",
				"Accept the new checkout page",
				"Review",
				"/tasks/CTV-2/accept",
			],
			["question", "Mira has a question", "Answer", "/tasks/CTV-3/questions"],
			["help", "Theo needs your help", "Answer", "/tasks/CTV-4/help"],
			[
				"integration",
				"Add the photos to your project",
				"Add",
				"/tasks/CTV-5/accept",
			],
		] as const;
		expected.forEach(([kind, title, word, route], i) => {
			const one = rows[i] as HTMLElement;
			expect(within(one).getByText(title)).toBeTruthy();
			expect(within(one).getByText(`${kind} line`)).toBeTruthy();
			const link = within(one).getByRole("link", { name: word });
			expect(link.getAttribute("href")).toBe(route);
		});
		picturesOpenTheirAgents(list, TEAM.agents);
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
				"All 3 of Catervas’s checks passed.",
			),
		).toBeTruthy();
		expect(
			s
				.calls("query")
				.filter((q) => q.params.name === "task.checks")
				.map((q) => q.params.params),
		).toEqual([{ task_id: "CTV-2" }]);
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
					task_id: "CTV-3",
					kind: "question",
					agent_id: "mira",
					title: "the launch post",
					line: "question line",
				},
				{
					task_id: "CTV-9",
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
					task_id: "CTV-14",
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
				"To create issue, for CTV-14 Sold-out badge on the menu. Theo waits until you decide.",
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
				.getByRole("link", { name: "CTV-14 Sold-out badge on the menu" })
				.getAttribute("href"),
		).toBe("/tasks/CTV-14");
		await expectNoAxeViolations(container);
	});

	it("links_the_waiting_rows_to_settings", async () => {
		const { container } = await today({
			waiting: [
				{
					task_id: "CTV-22",
					kind: "preview_missing",
					agent_id: "iris",
					title: "The Order again button",
					line: "iris needs to know how to open your app",
				},
				{
					task_id: "CTV-23",
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
		).toBe("/settings/your-app");
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
					task_id: "CTV-24",
					kind: "designer_needs_browser",
					agent_id: "iris",
					title: "A bigger basket",
					line: "Iris has Playwright off, so Catervas gives Iris no work. Turn Playwright on for Iris on the Team page",
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
		expect(link.getAttribute("href")).toBe("/settings/account");
		await expectNoAxeViolations(container);
	});

	/** The moved list's cards, newest first as the page draws them. */
	const cards = async () =>
		within(
			await screen.findByRole("list", { name: en.movedTitle }),
		).getAllByRole("listitem");
	/** `n` moves by Theo on an earlier day, `minute` apart, oldest first as the daemon answers. */
	const moves = (n: number) =>
		Array.from({ length: n }, (_, i) => ({
			at: `2026-01-01T10:${String(i).padStart(2, "0")}:00Z`,
			by: "theo",
			line: `Theo did thing ${i + 1}.`,
		}));

	it("says_what_moved", async () => {
		const now = new Date().toISOString().replace(/\.\d+Z$/, "Z");
		const { container, s } = await today({
			moved: [
				{
					at: "2026-01-01T21:40:00Z",
					by: "catervas",
					line: "Catervas stopped the X post for 09:00: its plan ended.",
				},
				{
					at: now,
					by: "human",
					line: "You accepted “Menu photos”",
				},
				{
					at: now,
					by: "theo",
					line: "Theo finished the checkout page and asked Ada to review it.",
				},
			],
		});
		const [newest, middle, oldest] = (await cards()) as HTMLElement[];
		// The newest is first: the daemon answers oldest first and the page turns it round.
		expect(
			within(newest as HTMLElement).getByText(
				"Theo finished the checkout page and asked Ada to review it.",
			),
		).toBeTruthy();
		// An agent's card: the picture, which opens the agent's page, the name, the role tag, the time.
		picturesOpenTheirAgents(newest as HTMLElement, TEAM.agents);
		expect(
			within(newest as HTMLElement).getByText("Theo", { selector: "strong" }),
		).toBeTruthy();
		expect(within(newest as HTMLElement).getByTitle("Developer")).toBeTruthy();
		const time = within(newest as HTMLElement).getByText(now.slice(11, 16));
		expect(time.tagName).toBe("TIME");
		expect(time.getAttribute("datetime")).toBe(now);
		expect(
			within(middle as HTMLElement).getByText("You", { selector: "strong" }),
		).toBeTruthy();
		expect(
			within(oldest as HTMLElement).getByText("Catervas", {
				selector: "strong",
			}),
		).toBeTruthy();
		expect(
			(oldest as HTMLElement).querySelector("img")?.getAttribute("alt"),
		).toBe("");
		// What moved on an earlier day says so.
		expect(
			within(oldest as HTMLElement).getByText("Yesterday 21:40"),
		).toBeTruthy();
		// Since yesterday: 24 hours back from now.
		const asked = s.calls("query").find((q) => q.params.name === "moved.since");
		const since = Date.parse(
			(asked?.params.params as { since: string } | undefined)?.since ?? "",
		);
		expect(Math.abs(Date.now() - 86_400_000 - since)).toBeLessThan(60_000);
		await expectNoAxeViolations(container);
	});

	it("shows_three_moves_then_more", async () => {
		const { container } = await today({ moved: moves(7) });
		const lines = async () =>
			(await cards()).map((card) => card.textContent ?? "");
		const first = await cards();
		expect(first).toHaveLength(3);
		expect(first[0]?.textContent).toContain("Theo did thing 7.");
		expect(first[1]?.textContent).toContain("Theo did thing 6.");
		expect(first[2]?.textContent).toContain("Theo did thing 5.");
		const button = screen.getByRole("button", { name: "Show more (4 more)" });
		button.focus();
		await expectNoAxeViolations(container);

		fireEvent.click(button);
		const six = await lines();
		expect(six).toHaveLength(6);
		expect(six.slice(0, 3)).toEqual(first.map((card) => card.textContent));
		expect(six[5]).toContain("Theo did thing 2.");
		const more = screen.getByRole("button", { name: "Show more (1 more)" });
		expect(document.activeElement).toBe(more);
		await expectNoAxeViolations(container);

		fireEvent.click(more);
		expect(await lines()).toHaveLength(7);
		const fewer = screen.getByRole("button", { name: "Show fewer" });
		expect(screen.queryByRole("button", { name: /Show more/ })).toBeNull();
		expect(document.activeElement).toBe(fewer);
		await expectNoAxeViolations(container);

		fireEvent.click(fewer);
		expect(await lines()).toHaveLength(3);
		expect(
			document.activeElement ===
				screen.getByRole("button", { name: "Show more (4 more)" }),
		).toBe(true);
		await expectNoAxeViolations(container);
	});

	it("shows_no_button_for_three_or_fewer", async () => {
		await today({ moved: moves(3) });
		expect(await cards()).toHaveLength(3);
		expect(screen.queryByRole("button", { name: /Show more/ })).toBeNull();
		expect(screen.queryByRole("button", { name: /Show fewer/ })).toBeNull();
	});

	it("names_an_agent_the_team_does_not_hold", async () => {
		await today({
			moved: [
				{
					at: "2026-01-01T10:00:00Z",
					by: "assignee",
					line: "assignee moved “Menu photos” to Review",
				},
			],
		});
		const [card] = (await cards()) as HTMLElement[];
		expect(
			within(card as HTMLElement).getByText("assignee", {
				selector: "strong",
			}),
		).toBeTruthy();
		expect((card as HTMLElement).querySelector("img")).toBeNull();
		expect(card?.querySelector("abbr")).toBeNull();
	});

	it("draws_the_person_and_catervas", async () => {
		const team = {
			...TEAM,
			agents: [
				...TEAM.agents,
				{
					id: "kai",
					display_name: "Kai",
					role: "marketing_specialist",
					status: "active",
				},
			],
		};
		await today({
			team,
			moved: [
				{ at: "2026-01-01T10:00:00Z", by: "kai", line: "Kai wrote a post." },
				{ at: "2026-01-01T10:01:00Z", by: "catervas", line: "Catervas did." },
				{ at: "2026-01-01T10:02:00Z", by: "human", line: "You did." },
			],
		});
		const [human, catervas, kai] = (await cards()) as HTMLElement[];
		const tile = (human as HTMLElement).querySelector("[aria-hidden='true']");
		expect(tile?.querySelector("svg")).toBeTruthy();
		expect((human as HTMLElement).querySelector("img")).toBeNull();
		const mark = (catervas as HTMLElement).querySelector("img");
		expect(mark?.getAttribute("alt")).toBe("");
		expect(mark?.getAttribute("src")).toContain("logo-mark");
		expect((kai as HTMLElement).querySelector("img")).toBeNull();
		expect(
			within(kai as HTMLElement).getByTitle("Marketing Specialist"),
		).toBeTruthy();
	});

	/** The row `waiting.list` gives while Kai's marketing plan waits on the owner. */
	const PLAN_ROW = {
		task_id: "CTV-31",
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
			"Approved in your plan MP-3. Catervas hands it to Buffer at 12:00.",
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
			"Approved in your plan MP-3. Catervas hands it to Buffer an hour before.",
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
		for (const row of [...going, ...over, ...asking]) {
			expect(within(row).getByRole("img", { name: "Kai" })).toBeTruthy();
			picturesOpenTheirAgents(row, MARKETING_TEAM.agents);
		}
	});

	it("what_catervas_cannot_show_opens_in_a_new_tab", async () => {
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
		// Besides the agent's picture, which opens its page, one link.
		expect(
			within(row as HTMLElement)
				.queryAllByRole("link")
				.filter((link) => !link.getAttribute("href")?.startsWith("/team/")),
		).toHaveLength(1);
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
				"If you allow it, Catervas sends it at its time, and it waits under Going out until then, with Stop.",
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
			"Catervas could not hand it to Buffer before its time.",
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

describe("a marketing plan's ads and their budget", () => {
	afterEach(() => vi.unstubAllGlobals());

	/** The one row, with the list's other rows left out. */
	async function rowOf(...waiting: unknown[]) {
		const { container, s } = await todayWith({
			waiting,
			team: MARKETING_TEAM,
		});
		const list = await screen.findByRole("list", { name: en.waitingList });
		const [row] = within(list).getAllByRole("listitem");
		return { container, s, row: row as HTMLElement };
	}

	it("today_offers_to_raise_or_end", async () => {
		const { container, row } = await rowOf(BUDGET_ROW);
		expect(
			screen.getByRole("heading", { name: "Waiting on you (1)" }),
		).toBeTruthy();
		// The row is Catervas's own, so it carries Catervas's picture; the plan's title is text.
		expect(within(row).getByRole("img", { name: "Catervas" })).toBeTruthy();
		expect(
			within(row).getByText(`Ads budget reached: ${TITLE}`, {
				selector: "strong",
			}),
		).toBeTruthy();
		expect(row.textContent).toContain(
			"Its campaign Bakery near me reached its budget: $150.00 of $150.00 USD. Catervas paused it.",
		);
		expect(row.textContent).toContain(
			"Google Ads: $318.65 of $450.00 USD spent",
		);
		expect(row.textContent).toContain("Ends Sunday 22 November");
		expect(container.querySelector("b")).toBeNull();
		// Catervas paused it: nothing is left running to stop at Google.
		expect(
			within(row).queryByRole("link", { name: "Open Google Ads" }),
		).toBeNull();
		for (const name of ["Raise the budget", "End the plan"])
			expect(within(row).getByRole("button", { name })).toBeTruthy();
		await expectNoAxeViolations(container);

		// The plan's own budget, and Google would not take the pause: it says what Google answered,
		// and where to stop the ads by hand.
		cleanup();
		const refused = await rowOf(PLAN_BUDGET_ROW);
		expect(refused.row.textContent).toContain(
			"Its ads reached their budget: $450.00 of $450.00 USD. Catervas could not pause them: Google answered “The service is currently unavailable.” Catervas tries again every 15 minutes; pause them in Google Ads.",
		);
		const ads = within(refused.row).getByRole("link", {
			name: "Open Google Ads",
		});
		expect(ads.getAttribute("href")).toBe("https://ads.google.com");
		expect(ads.getAttribute("target")).toBe("_blank");
		expect(ads.getAttribute("rel")).toContain("noopener");
		await expectNoAxeViolations(refused.container);

		// While the owner's raise is open, the row says so, and only "End the plan" is left.
		cleanup();
		const raising = await rowOf({ ...BUDGET_ROW, raising: "CTV-40" });
		expect(raising.row.textContent).toContain(
			"Catervas paused it. You asked Kai for a new version with a raised budget. It waits for you here when it is ready.",
		);
		expect(
			within(raising.row).queryByRole("button", { name: "Raise the budget" }),
		).toBeNull();
		expect(
			within(raising.row).getByRole("button", { name: "End the plan" }),
		).toBeTruthy();
	});

	it("the_row_speaks_of_the_cap_the_daemon_names", async () => {
		// The campaign's pause was refused and stands; the plan's own cap, reached after it, was
		// paused. The words are about the campaign's cap, not the newest.
		const { row } = await rowOf({
			...BUDGET_ROW,
			reason: REFUSED,
			caps: [
				BUDGET_ROW.cap,
				{ scope: "plan", spent: "450.00", budget: "450.00" },
			],
		});
		expect(row.textContent).toContain(
			"Its campaign Bakery near me reached its budget: $150.00 of $150.00 USD. Catervas could not pause it: Google answered “The service is currently unavailable.” Catervas tries again every 15 minutes; pause it in Google Ads.",
		);
		expect(row.textContent).not.toContain("$450.00 of $450.00");
	});

	it("end_sends_marketing_plan_end", async () => {
		const { s, row } = await rowOf(BUDGET_ROW);
		fireEvent.click(within(row).getByRole("button", { name: "End the plan" }));
		// The question is the plan page's own, so it asks for the plan.
		await answerQuery(s, "marketing_plan.get", RUNNING_PLAN);
		expect(
			s.calls("query").find((q) => q.params.name === "marketing_plan.get")
				?.params.params,
		).toEqual({ plan: "MP-3" });
		const dialog = await screen.findByRole("dialog", {
			name: "End this plan now?",
		});
		expect(dialog.textContent).toContain(
			"Catervas pauses its running ads within a minute. $318.65 is spent so far, by Google’s figures at 10:15.",
		);
		// Asking first: nothing is sent until it is confirmed.
		expect(s.calls("command")).toHaveLength(0);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "End the plan" }),
		);
		const ending = await sent(s);
		expect(ending.params).toEqual({
			command: { command: "marketing_plan_end", body: { plan: "MP-3" } },
		});
		await s.reply(ending, { said: "ended", events: [10] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("today_says_when_ads_keep_running", async () => {
		const { container, row } = await rowOf(RUNNING_ROW);
		expect(within(row).getByRole("img", { name: "Catervas" })).toBeTruthy();
		expect(
			within(row).getByText(`Ads still running: ${TITLE}`, {
				selector: "strong",
			}),
		).toBeTruthy();
		expect(row.textContent).toContain(
			"Catervas could not pause its ads: Google answered “The service is currently unavailable.” They keep running at Google until Sunday 22 November or their budget there. Pause them in Google Ads.",
		);
		expect(
			within(row)
				.getByRole("link", { name: "Open Google Ads" })
				.getAttribute("href"),
		).toBe("https://ads.google.com");
		// Nothing to raise: the plan ended.
		expect(within(row).queryAllByRole("button")).toHaveLength(0);
		await expectNoAxeViolations(container);
	});

	it("today_says_when_the_spend_cannot_be_read", async () => {
		const { container, row } = await rowOf(UNREAD_ROW);
		expect(
			within(row).getByText(`Can’t read the ad spend: ${TITLE}`, {
				selector: "strong",
			}),
		).toBeTruthy();
		expect(row.textContent).toContain(
			"Catervas can’t read its ad spend: Kai's sign-in to Google has ended; sign Kai in again on Kai's page. Any of its ads still running keep running at Google until Sunday 22 November or their budget there; pause them in Google Ads.",
		);
		// The last spend Catervas could read stays on the row, with when.
		expect(row.textContent).toContain(
			"Last read Thursday 12 November at 10:15: $318.65 of $450.00 USD",
		);
		expect(
			within(row).getByRole("link", { name: "Open Google Ads" }),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});

describe("a purchase order the Procurement Specialist suggests", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	/** Today with the rows waiting; the rows of the waiting list. */
	async function rowsOf(...waiting: unknown[]) {
		const { container, s } = await todayWithOrders({ waiting });
		const list = await screen.findByRole("list", { name: en.waitingList });
		return {
			container,
			s,
			rows: within(list).getAllByRole("listitem") as HTMLElement[],
		};
	}

	/** The cells of each row of an order's table, header first. */
	const cells = (table: HTMLElement) =>
		within(table)
			.getAllByRole("row")
			.map((row) =>
				Array.from(row.querySelectorAll("th, td"), (cell) => cell.textContent),
			);

	it("shows_an_order_with_its_lines_total_and_host", async () => {
		const { container, rows } = await rowsOf(ORDER_ROW);
		const [row] = rows as [HTMLElement];
		expect(
			screen.getByRole("heading", { name: "Waiting on you (1)" }),
		).toBeTruthy();

		// Its words, and the task it comes from.
		expect(
			within(row).getByText("Ivo set up an order from Pie Box Pros", {
				selector: "strong",
			}),
		).toBeTruthy();
		expect(row.textContent).toContain(
			"For CTV-31 Find a supplier for 500 pie boxes.",
		);
		expect(
			within(row)
				.getByRole("link", { name: "CTV-31 Find a supplier for 500 pie boxes" })
				.getAttribute("href"),
		).toBe("/tasks/CTV-31");
		expect(within(row).getByRole("img", { name: "Ivo" })).toBeTruthy();

		// Every line, with its amounts and their currency.
		expect(cells(within(row).getByRole("table"))).toEqual([
			["Item", "Quantity", "Price each", "Line total"],
			[
				"Printed pie box, 10 × 10 × 2.5 in, white, your logo in one colour",
				"500 boxes",
				"2.40 USD",
				"1,200.00 USD",
			],
			["Printing plate for your logo", "1", "85.00 USD", "85.00 USD"],
			["Delivery to Corner Bakery", "1", "165.00 USD", "165.00 USD"],
		]);
		expect(
			within(row).getByText("1,450.00 USD").closest("p")?.textContent,
		).toBe("Total 1,450.00 USD once");
		expect(
			within(row)
				.getAllByRole("term")
				.map((term) => term.textContent),
		).toEqual(["Contact", "Delivery", "Terms"]);
		expect(within(row).getByText("Dana Ruiz, sales, +1 555 0142")).toBeTruthy();
		expect(within(row).getByText(ORDER_ROW.delivery)).toBeTruthy();
		expect(within(row).getByText(ORDER_ROW.terms)).toBeTruthy();
		expect(
			within(row).getByText(
				"Ivo wrote these from the seller’s pages. Check them with the seller before you pay.",
			),
		).toBeTruthy();

		// The seller's page: the site in bold, the address as text, and a link that cannot reach this page.
		expect(
			within(row).getByText("pieboxpros.com", { selector: "strong code" }),
		).toBeTruthy();
		expect(within(row).getByText(ORDER_ROW.url).closest("a")).toBeNull();
		const open = within(row).getByRole("link", { name: "Open" });
		expect(open.getAttribute("href")).toBe(ORDER_ROW.url);
		expect(open.getAttribute("target")).toBe("_blank");
		expect(open.getAttribute("rel")).toContain("noopener");
		expect(open.getAttribute("rel")).toContain("noreferrer");
		expect(
			within(row).getByText("Check it is Pie Box Pros’s page before you pay."),
		).toBeTruthy();
		expect(within(row).queryByText(en.scriptWarningWhat)).toBeNull();

		// Why is the agent's own words, in a frame that says so; the day it closes by itself is plain.
		const why = within(row).getByRole("group", { name: "Why, in Ivo’s words" });
		expect(why.getAttribute("data-trust")).toBe("untrusted");
		expect(why.textContent).toBe(ORDER_ROW.why);
		expect(
			within(row).getByText(
				"If you don’t decide by 25 November, the order closes by itself.",
			),
		).toBeTruthy();
		expect(
			within(within(row).getByRole("group", { name: /^Ivo set up an order/ }))
				.getAllByRole("button")
				.map((button) => button.textContent),
		).toEqual(["Approve, I’ll place it myself", "Reject"]);
		expect(
			within(row).getByRole("button", { name: "Download PO-12.xlsx" }),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("says_what_it_holds_when_there_are_many_lines_or_no_page", async () => {
		const six = {
			...LONG_ORDER_ROW,
			order: 16,
			lines: LONG_ORDER_ROW.lines.slice(0, 6),
		};
		const { rows } = await rowsOf(LONG_ORDER_ROW, six);
		const [seven, sixLines] = rows as [HTMLElement, HTMLElement];

		// Five lines and how many more there are; a monthly order says so.
		expect(
			within(within(seven).getByRole("table")).getAllByRole("row"),
		).toHaveLength(1 + 5);
		expect(
			within(seven).getByText("and 2 more lines in the order"),
		).toBeTruthy();
		expect(
			within(sixLines).getByText("and 1 more line in the order"),
		).toBeTruthy();
		expect(
			within(seven).getByText("140.00 USD", { exact: false }).closest("p")
				?.textContent,
		).toBe("Total 140.00 USD a month");

		// An order with no page for the seller says so, and has nothing to open or to check.
		expect(within(seven).getByText(en.orderNoPage)).toBeTruthy();
		// A fact the agent left empty has no line.
		expect(within(seven).queryAllByRole("term")).toHaveLength(0);
		expect(within(seven).queryByRole("link", { name: "Open" })).toBeNull();
		expect(within(seven).queryByText(/Check it is/)).toBeNull();
		expect(within(seven).queryByText("pieboxpros.com")).toBeNull();
	});

	it("shows_what_the_agent_wrote_as_text", async () => {
		const plain = { ...ORDER_ROW, order: 17, seller: "Plain Seller" };
		const http = {
			...ORDER_ROW,
			order: 18,
			seller: "Http Seller",
			url: "http://pieboxpros.com/x",
		};
		const script = { ...SCRIPT_ORDER_ROW };
		const { container, rows } = await rowsOf(WRITTEN_ROW, plain, http, script);
		const [written, , insecure, scripted] = rows as [
			HTMLElement,
			HTMLElement,
			HTMLElement,
			HTMLElement,
		];
		const markup = "<b>not bold</b>";

		// Markup stays text, and a character that reorders text is written out.
		expect(container.querySelector("b")).toBeNull();
		expect(
			within(written).getByText(
				`Ivo set up an order from Pie Box ${markup} Pros\\u{202e}`,
				{ selector: "strong" },
			),
		).toBeTruthy();
		expect(cells(within(written).getByRole("table"))[1]).toEqual([
			`Box ${markup}\\u{202e}`,
			"5 bo\\u{202e}xes",
			"1.00 USD",
			"5.00 USD",
		]);
		for (const text of [
			`Dana ${markup}\\u{202e}`,
			`Soon ${markup}\\u{202e}`,
			`Net 30 ${markup}\\u{202e}`,
			"https://pieboxpros.com/a\\u{202e}b",
		])
			expect(within(written).getByText(text)).toBeTruthy();
		const why = within(written).getByRole("group", {
			name: "Why, in Ivo’s words",
		});
		expect(why.textContent).toBe(`Cheapest ${markup} and \\u{202e}this`);
		expect(why.getAttribute("data-trust")).toBe("untrusted");
		expect(written.textContent).toContain(
			`Check it is Pie Box ${markup} Pros\\u{202e}’s page`,
		);

		// Only a plain https address is a link.
		expect(within(insecure).queryByRole("link", { name: "Open" })).toBeNull();
		expect(within(insecure).queryByText("Open")).toBeNull();
		expect(within(insecure).getByText("http://pieboxpros.com/x")).toBeTruthy();

		// A name written in another alphabet is warned of, in its plain form, where the other is not.
		expect(within(written).queryByText(en.scriptWarningWhat)).toBeNull();
		expect(within(scripted).getByText(en.scriptWarningWhat)).toBeTruthy();
		expect(
			within(scripted).getByText("xn--ulne-m9d.com", {
				selector: "strong code",
			}),
		).toBeTruthy();
		expect(within(scripted).getByText(SCRIPT_ORDER_ROW.url)).toBeTruthy();
	});

	it("the_comparison_opens_in_an_untrusted_frame", async () => {
		const { container, s, rows } = await rowsOf(ORDER_ROW);
		const [row] = rows as [HTMLElement];
		const asked = () =>
			s
				.calls("query")
				.filter((q) => q.params.name === "purchase_order.evaluation");

		// Nothing is read until the owner opens it.
		expect(asked()).toHaveLength(0);
		fireEvent.click(within(row).getByText("Read the comparison"));
		await answerQuery(s, "purchase_order.evaluation", { text: COMPARISON });
		expect(asked().map((q) => q.params.params)).toEqual([{ order: 12 }]);

		// The note is the agent's: its markup is text in a frame that says whose words they are.
		const frame = await within(row).findByRole("group", {
			name: "Read the comparison",
		});
		expect(frame.getAttribute("data-trust")).toBe("untrusted");
		expect(frame.textContent).toBe(COMPARISON.replace("\u202e", "\\u{202e}"));
		expect(container.querySelector("b")).toBeNull();
	});

	it("says_so_when_the_comparison_cannot_be_read", async () => {
		const { s, rows } = await rowsOf(ORDER_ROW);
		const [row] = rows as [HTMLElement];
		fireEvent.click(within(row).getByText("Read the comparison"));
		const [frame] = await waitFor(() => {
			const asked = s
				.calls("query")
				.filter((q) => q.params.name === "purchase_order.evaluation");
			if (asked.length === 0) throw new Error("not asked yet");
			return asked;
		});
		await s.fail(frame as never, -32002, "there is no such file");
		expect(await within(row).findByText(en.orderComparisonGone)).toBeTruthy();
		expect(
			within(row).queryByRole("group", { name: "Read the comparison" }),
		).toBeNull();
	});

	it("downloads_the_order", async () => {
		const made: Blob[] = [];
		const gone: string[] = [];
		const clicked: { download: string; href: string }[] = [];
		URL.createObjectURL = (blob: Blob | MediaSource) => {
			made.push(blob as Blob);
			return "blob:order-12";
		};
		URL.revokeObjectURL = (url: string) => {
			gone.push(url);
		};
		const click = vi
			.spyOn(HTMLAnchorElement.prototype, "click")
			.mockImplementation(function (this: HTMLAnchorElement) {
				clicked.push({ download: this.download, href: this.href });
			});
		try {
			const { s, rows } = await rowsOf(ORDER_ROW);
			const [row] = rows as [HTMLElement];
			fireEvent.click(
				within(row).getByRole("button", { name: "Download PO-12.xlsx" }),
			);

			// The daemon gives the file it wrote, and the browser keeps it under the order's name.
			const [frame] = await waitFor(() => {
				const asked = s.calls("purchase_order.file");
				if (asked.length === 0) throw new Error("no file was asked for");
				return asked;
			});
			expect((frame as { params: object }).params).toEqual({ order: 12 });
			await s.reply(frame as never, {
				media_type:
					"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
				base64: btoa("PK order bytes"),
				name: "PO-12.xlsx",
			});
			await waitFor(() => expect(clicked).toHaveLength(1));
			expect(clicked).toEqual([
				{ download: "PO-12.xlsx", href: "blob:order-12" },
			]);
			expect(made).toHaveLength(1);
			expect((made[0] as Blob).type).toBe(
				"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
			);
			expect(
				await new Promise((done) => {
					const reader = new FileReader();
					reader.onload = () => done(reader.result);
					reader.readAsText(made[0] as Blob);
				}),
			).toBe("PK order bytes");
			await waitFor(() => expect(gone).toEqual(["blob:order-12"]));
			expect(within(row).queryByRole("alert")).toBeNull();

			// A file that is gone is said, and nothing is kept.
			fireEvent.click(
				within(row).getByRole("button", { name: "Download PO-12.xlsx" }),
			);
			const second = await waitFor(() => {
				const asked = s.calls("purchase_order.file");
				if (asked.length < 2) throw new Error("not asked again");
				return asked[1];
			});
			await s.fail(second as never, -32002, "there is no such file");
			expect((await within(row).findByRole("alert")).textContent).toBe(
				en.orderFileGone,
			);
			expect(clicked).toHaveLength(1);
		} finally {
			click.mockRestore();
		}
	});
});

describe("the renewals coming up", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	/** The rows of the renewals' list. */
	const renewalRows = async () =>
		within(
			await screen.findByRole("list", { name: en.renewalsTitleNone }),
		).getAllByRole("listitem") as HTMLElement[];

	it("lists_the_renewals_after_what_waits", async () => {
		const question = {
			task_id: "CTV-2",
			kind: "question",
			agent_id: "mira",
			title: "Pie week",
			line: "Should pie pre-orders close on 22 or 23 November?",
		};
		const { container } = await todayWithOrders({
			waiting: [question],
			renewals: RENEWALS,
		});
		const heading = await screen.findByRole("heading", {
			name: "Renewals coming up (2)",
		});
		// They come after "Waiting on you", in a list of their own.
		const waiting = screen.getByRole("heading", { name: "Waiting on you (1)" });
		expect(
			waiting.compareDocumentPosition(heading) &
				Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		const [vercel, mailchimp] = await renewalRows();
		expect(
			within(vercel as HTMLElement).getByText("Vercel renews on 15 November", {
				selector: "strong",
			}),
		).toBeTruthy();
		expect((vercel as HTMLElement).textContent).toContain(
			"Decide by 16 October if you want to change or cancel it. It is in the register Ivo keeps.",
		);
		// The day to decide by is "today" or "tomorrow" when it is, but the day it renews is always a date.
		expect(
			within(mailchimp as HTMLElement).getByText(
				"Mailchimp renews on 27 October",
			),
		).toBeTruthy();
		expect((mailchimp as HTMLElement).textContent).toContain(
			"Decide by today if you want to change or cancel it.",
		);
		for (const row of [vercel, mailchimp])
			expect(
				within(
					within(row as HTMLElement).getByRole("group", {
						name: /renews on/,
					}),
				)
					.getAllByRole("button")
					.map((button) => button.textContent),
			).toEqual(["Ask for a review", "Dismiss"]);
		await expectNoAxeViolations(container);
	});

	it("says_how_many_dates_cannot_be_read", async () => {
		// With renewals open: the line follows them.
		await todayWithOrders({ renewals: RENEWALS });
		expect(
			await screen.findByText(
				"2 rows in the register have a renewal date Catervas can’t read, so Catervas can’t remind you of them.",
			),
		).toBeTruthy();
	});

	it("says_how_many_dates_cannot_be_read_with_no_renewal_open", async () => {
		await todayWithOrders({ renewals: { open: [], unreadable: 3 } });
		// The line stands alone under a heading with no count.
		expect(
			await screen.findByRole("heading", { name: "Renewals coming up" }),
		).toBeTruthy();
		expect(
			screen.getByText(
				"3 rows in the register have a renewal date Catervas can’t read, so Catervas can’t remind you of them.",
			),
		).toBeTruthy();
		expect(
			screen.queryByRole("list", { name: en.renewalsTitleNone }),
		).toBeNull();
	});

	it("says_one_date_cannot_be_read_in_the_singular", async () => {
		await todayWithOrders({ renewals: { open: [], unreadable: 1 } });
		expect(
			await screen.findByText(
				"1 row in the register has a renewal date Catervas can’t read, so Catervas can’t remind you of it.",
			),
		).toBeTruthy();
	});

	it("says_nothing_when_nothing_is_due_and_every_date_reads", async () => {
		await todayWithOrders({ renewals: { open: [], unreadable: 0 } });
		await screen.findByRole("heading", { name: "Waiting on you (0)" });
		expect(screen.queryByText(/Renewals coming up/)).toBeNull();
	});

	it("says_nothing_about_dates_when_every_date_reads", async () => {
		await todayWithOrders({ renewals: { ...RENEWALS, unreadable: 0 } });
		await screen.findByRole("heading", { name: "Renewals coming up (2)" });
		expect(
			screen.queryByText(/in the register have|in the register has/),
		).toBeNull();
	});

	it("shows_a_vendor_as_text", async () => {
		const vendor = "Ver\u202ecel <b>not bold</b>";
		await todayWithOrders({
			renewals: {
				open: [renewal(3, vendor, "2026-11-15", "2026-10-16")],
				unreadable: 0,
			},
		});
		const [row] = await renewalRows();
		const shown = "Ver\\u{202e}cel <b>not bold</b>";
		expect(
			within(row as HTMLElement).getByText(`${shown} renews on 15 November`, {
				selector: "strong",
			}),
		).toBeTruthy();
		expect((row as HTMLElement).querySelector("b")).toBeNull();

		// The request the owner reads says it the same way, so that what is sent is what was read.
		fireEvent.click(
			within(row as HTMLElement).getByRole("button", {
				name: "Ask for a review",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: `Ask the team to review ${shown}?`,
		});
		expect(
			(
				within(dialog).getByRole("textbox", {
					name: "Your request",
				}) as HTMLTextAreaElement
			).value,
		).toBe(
			`Review ${shown} before it renews on 15 November; decide by 16 October.`,
		);
	});

	it("dismiss_acts_at_once", async () => {
		const { s } = await todayWithOrders({ renewals: RENEWALS });
		const [vercel] = await renewalRows();
		fireEvent.click(
			within(vercel as HTMLElement).getByRole("button", { name: "Dismiss" }),
		);

		// No dialog: the command goes at once, and the list is read again when it went through.
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: { command: "renewal_dismiss", body: { renewal: 7 } },
		});
		expect(screen.queryByRole("dialog")).toBeNull();
		const asked = () =>
			s.calls("query").filter((q) => q.params.name === "renewals.list").length;
		expect(asked()).toBe(1);
		await s.reply(sent, { said: "dismissed", events: [80] });
		await waitFor(() => expect(asked()).toBe(2));
	});

	it("dismissing_twice_is_said_in_words", async () => {
		const { s } = await todayWithOrders({ renewals: RENEWALS });
		const [vercel] = await renewalRows();
		fireEvent.click(
			within(vercel as HTMLElement).getByRole("button", { name: "Dismiss" }),
		);
		const sent = await sentCommand(s);
		await s.reply(sent, {
			error: {
				kind: "refused",
				detail: "renewal_dismissed: renewal 7 was dismissed already",
			},
		});
		expect(
			(await within(vercel as HTMLElement).findByRole("alert")).textContent,
		).toBe(en.refuseRenewalDismissed);
	});

	it("ask_for_a_review_files_the_edited_request_then_dismisses", async () => {
		const { container, s } = await todayWithOrders({ renewals: RENEWALS });
		const [vercel] = await renewalRows();
		fireEvent.click(
			within(vercel as HTMLElement).getByRole("button", {
				name: "Ask for a review",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Ask the team to review Vercel?",
		});
		expect(
			within(dialog).getByText(
				"It goes to the team as your request, in your words. Change them if you like.",
			),
		).toBeTruthy();
		const text = within(dialog).getByRole("textbox", {
			name: "Your request",
		}) as HTMLTextAreaElement;
		expect(text.value).toBe(
			"Review Vercel before it renews on 15 November; decide by 16 October.",
		);
		expect(
			within(dialog).getByText(
				"Mira reads every request and asks you if anything is unclear.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		// Closing sends nothing.
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("request.file")).toHaveLength(0);
		expect(s.calls("command")).toHaveLength(0);

		// The owner's own words are what is filed, and the renewal is dismissed only after.
		fireEvent.click(
			within(vercel as HTMLElement).getByRole("button", {
				name: "Ask for a review",
			}),
		);
		const again = await screen.findByRole("dialog", {
			name: "Ask the team to review Vercel?",
		});
		// A request with no words cannot be sent.
		fireEvent.change(
			within(again).getByRole("textbox", { name: "Your request" }),
			{ target: { value: "   " } },
		);
		expect(
			(
				within(again).getByRole("button", {
					name: en.requestSend,
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);
		fireEvent.change(
			within(again).getByRole("textbox", { name: "Your request" }),
			{
				target: {
					value: "  Is Vercel still worth $20 a month? Cancel it if not.  ",
				},
			},
		);
		fireEvent.click(
			within(again).getByRole("button", { name: en.requestSend }),
		);
		const filed = await waitFor(() => {
			const asked = s.calls("request.file");
			if (asked.length === 0) throw new Error("no request was filed");
			return asked[0];
		});
		expect((filed as { params: object }).params).toEqual({
			text: "Is Vercel still worth $20 a month? Cancel it if not.",
		});
		expect(s.calls("command")).toHaveLength(0);
		await s.reply(filed as never, { task_id: "CTV-40" });
		const dismissed = await sentCommand(s);
		expect(dismissed.params).toEqual({
			command: { command: "renewal_dismiss", body: { renewal: 7 } },
		});
		const before = s
			.calls("query")
			.filter((q) => q.params.name === "renewals.list").length;
		await s.reply(dismissed, { said: "dismissed", events: [81] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		await waitFor(() =>
			expect(
				s.calls("query").filter((q) => q.params.name === "renewals.list")
					.length,
			).toBe(before + 1),
		);
	});

	it("the_review_request_names_days_not_today", async () => {
		await todayWithOrders({ renewals: RENEWALS });
		const [, mailchimp] = await renewalRows();
		// The row says "today"; the request is read later, so it says the day.
		expect((mailchimp as HTMLElement).textContent).toContain("Decide by today");
		fireEvent.click(
			within(mailchimp as HTMLElement).getByRole("button", {
				name: "Ask for a review",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Ask the team to review Mailchimp?",
		});
		expect(
			(
				within(dialog).getByRole("textbox", {
					name: "Your request",
				}) as HTMLTextAreaElement
			).value,
		).toBe(
			"Review Mailchimp before it renews on 27 October; decide by 26 October.",
		);
	});

	it("a_refused_request_dismisses_nothing", async () => {
		const { s } = await todayWithOrders({ renewals: RENEWALS });
		const [vercel] = await renewalRows();
		fireEvent.click(
			within(vercel as HTMLElement).getByRole("button", {
				name: "Ask for a review",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Ask the team to review Vercel?",
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.requestSend }),
		);
		const filed = await waitFor(() => {
			const asked = s.calls("request.file");
			if (asked.length === 0) throw new Error("no request was filed");
			return asked[0];
		});
		await s.fail(filed as never, -32002, "refused", {
			errors: [{ path: "", message: "too short", code: "too_short" }],
		});

		// The refusal is said, the dialog stays, and no renewal was dismissed.
		expect((await within(dialog).findByRole("alert")).textContent).toBe(
			en.requestTooShort,
		);
		expect(s.calls("command")).toHaveLength(0);

		// Sent again with better words, it goes through and the renewal is dismissed once.
		fireEvent.change(
			within(dialog).getByRole("textbox", { name: "Your request" }),
			{
				target: { value: "Please review Vercel before it renews." },
			},
		);
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.requestSend }),
		);
		const second = await waitFor(() => {
			const asked = s.calls("request.file");
			if (asked.length < 2) throw new Error("not filed again");
			return asked[1];
		});
		await s.reply(second as never, { task_id: "CTV-41" });
		const dismissed = await sentCommand(s);
		await s.reply(dismissed, { said: "dismissed", events: [82] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(1);
	});

	it("a_dismissal_that_is_refused_after_the_request_is_asked_again_alone", async () => {
		const { s } = await todayWithOrders({ renewals: RENEWALS });
		const [vercel] = await renewalRows();
		fireEvent.click(
			within(vercel as HTMLElement).getByRole("button", {
				name: "Ask for a review",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Ask the team to review Vercel?",
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.requestSend }),
		);
		const filed = await waitFor(() => {
			const asked = s.calls("request.file");
			if (asked.length === 0) throw new Error("no request was filed");
			return asked[0];
		});
		await s.reply(filed as never, { task_id: "CTV-42" });
		const first = await sentCommand(s);
		await s.reply(first, {
			error: { kind: "failed", detail: "the log could not be written" },
		});
		expect(await within(dialog).findByRole("alert")).toBeTruthy();

		// The request went through already: only the dismissal is sent again.
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.requestSend }),
		);
		const second = await sentCommand(s, 2);
		expect(s.calls("request.file")).toHaveLength(1);
		await s.reply(second, { said: "dismissed", events: [83] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});
});

describe("a data pipeline waiting on the owner", () => {
	afterEach(() => vi.useRealTimers());

	const rowOf = async (name: string) => {
		const list = await screen.findByRole("list", { name: en.waitingList });
		const found = within(list)
			.getAllByRole("listitem")
			.find((one) =>
				within(one).queryByText(new RegExp(`data source: ${name}`)),
			);
		if (!found) throw new Error(`no row for ${name}`);
		return found;
	};

	it("shows_why_the_owner_is_asked", async () => {
		await todayWith({
			waiting: [PAID_ROW, DATA_ROW, UNDECIDED_ROW],
			team: PIPELINE_TEAM,
		});
		const paid = await rowOf("Firecrawl");
		expect(
			within(paid).getByText("Ivo asks for a data source: Firecrawl", {
				selector: "strong",
			}),
		).toBeTruthy();
		expect(within(paid).getByText(/Ivo goes on without it\./)).toBeTruthy();
		const why = within(paid).getByRole("group", {
			name: "Why it comes to you",
		});
		// Catervas's sentences, in their order, then the Product Manager's reason.
		expect(why.textContent).toBe(
			`It costs money.The Product Manager asks you:It needs a paid plan, so it is your call.\\u{202e} ${PIPELINE_MARKUP}`,
		);
		const shippo = await rowOf("Shippo");
		expect(
			within(shippo).getByRole("group", { name: "Why it comes to you" })
				.textContent,
		).toContain("It sends your data to Shippo.The Product Manager asks you:");
		expect(within(shippo).queryByText("It costs money.")).toBeNull();
		// Catervas passed it on at once: the cost says why, and there is no reason to show.
		const undecided = await rowOf("Azure prices");
		const unclear = within(undecided).getByRole("group", {
			name: "Why it comes to you",
		});
		expect(unclear.textContent).toBe("Its cost is not known.");
		expect(within(undecided).queryByText(en.pipelineAsks)).toBeNull();
	});

	it("shows_what_the_agent_wrote_as_text", async () => {
		const { container } = await todayWith({
			waiting: [PAID_ROW, DATA_ROW, SCRIPT_PIPELINE_ROW, ODD_ADDRESS_ROW],
			team: PIPELINE_TEAM,
		});
		const paid = await rowOf("Firecrawl");
		expect(
			within(paid).getByText("firecrawl.dev", { selector: "strong" }),
		).toBeTruthy();
		for (const name of [
			"What it would give Ivo",
			"Why, in Ivo’s words",
			en.pipelineAsks,
		]) {
			const frame = within(paid).getByRole("group", { name });
			expect(frame.tagName).toBe("FIELDSET");
			expect(frame.getAttribute("data-trust")).toBe("untrusted");
		}
		// The agent's and the Product Manager's words are framed as theirs; markup stays text.
		const what = within(paid).getByRole("group", {
			name: "What it would give Ivo",
		});
		expect(what.getAttribute("data-trust")).toBe("untrusted");
		expect(what.textContent).toContain(`.\\u{202e} ${PIPELINE_MARKUP}`);
		expect(
			within(paid)
				.getByRole("group", { name: "Why, in Ivo’s words" })
				.getAttribute("data-trust"),
		).toBe("untrusted");
		expect(container.querySelector("b")).toBeNull();
		// The address is text, with a link to open it that leaves nothing behind.
		expect(
			within(paid)
				.getByText("https://www.firecrawl.dev/pricing", {
					selector: "code",
				})
				.closest("a"),
		).toBeNull();
		const open = within(paid).getByRole("link", { name: "Open" });
		expect(open.getAttribute("href")).toBe("https://www.firecrawl.dev/pricing");
		expect(open.getAttribute("target")).toBe("_blank");
		expect(open.getAttribute("rel")).toBe("noopener noreferrer");
		// An address that is not a web page is text alone.
		const odd = await rowOf("Odd");
		expect(within(odd).getByText("javascript:alert(1)")).toBeTruthy();
		expect(within(odd).queryByRole("link", { name: "Open" })).toBeNull();
		// The account line only when one is needed; the closing words always.
		expect(within(paid).getByText(en.pipelineAccount)).toBeTruthy();
		const shippo = await rowOf("Shippo");
		expect(within(shippo).queryByText(en.pipelineAccount)).toBeNull();
		for (const row of [paid, shippo]) {
			expect(
				within(row).getByText(/Ivo filled these in from the source’s pages/),
			).toBeTruthy();
			expect(within(row).getByText(en.pipelineNothingYet)).toBeTruthy();
		}
		// Another alphabet is warned of, a plain name is not.
		const script = await rowOf("Ulіne");
		expect(within(script).getByText(en.scriptWarningWhat)).toBeTruthy();
		expect(within(paid).queryByText(en.scriptWarningWhat)).toBeNull();
		await expectNoAxeViolations(container);
	});
});

describe("suggested connections", () => {
	afterEach(() => vi.unstubAllGlobals());

	const only = (...agents: ReturnType<typeof agent>[]) => ({
		...TEAM,
		agents,
	});
	const mira = agent("mira", "Mira", "product_manager", "product-manager");
	const ada = agent("ada", "Ada", "architect", "architect");
	const theo = agent("theo", "Theo", "software_developer", "developer");
	const pmKit = (...connectors: unknown[]) => ({
		role: "product_manager",
		connectors,
	});
	const items = () =>
		within(
			screen.getByRole("list", { name: en.suggestionsTitle }),
		).getAllByRole("listitem");

	it("suggests_a_kit_service_an_active_agent_lacks", async () => {
		await today({
			team: only(mira),
			kits: [pmKit(KIT_NOTION)],
			hiddenSuggestions: [{ agent: "ada", server: "notion" }],
		});
		const [row] = items() as [HTMLElement];
		expect(items()).toHaveLength(1);
		expect(
			screen.getByRole("heading", { name: en.suggestionsTitle }),
		).toBeTruthy();
		expect(row.textContent).toContain("Notion");
		expect(row.textContent).toContain(
			"Mira · Notion is where your team keeps docs, notes and plans.",
		);
		expect(within(row).getByText(KIT_NOTION.why)).toBeTruthy();
		const connect = within(row).getByRole("link", {
			name: en.suggestionsConnect,
		});
		expect(connect.getAttribute("href")).toBe("/team/mira?connect=notion");
		const describedBy = connect.getAttribute("aria-describedby") ?? "";
		expect(document.getElementById(describedBy)?.textContent).toBe("Notion");
		expect(within(row).queryByRole("img", { name: "Mira" })).toBeNull();
	});

	it("lists_an_asked_connection_first_with_its_reason", async () => {
		const asked = {
			agent: "mira",
			server: "linear",
			why: "I need it to track the bugs the team files.",
		};
		await today({
			team: only(mira),
			kits: [pmKit(KIT_NOTION, KIT_LINEAR)],
			requestedConnections: [asked],
		});
		const [first, second] = items() as [HTMLElement, HTMLElement];
		expect(first.textContent).toContain("Linear");
		expect(first.textContent).toContain(
			"Mira asks: I need it to track the bugs the team files.",
		);
		expect(
			within(first)
				.getByRole("link", { name: en.suggestionsConnect })
				.getAttribute("href"),
		).toBe("/team/mira?connect=linear");
		expect(second.textContent).toContain("Notion");
		expect(second.textContent).toContain(KIT_NOTION.about);
	});

	it("keeps_a_requested_connection_hidden_when_the_user_said_not_now", async () => {
		await today({
			team: only(mira),
			kits: [pmKit(KIT_NOTION, KIT_LINEAR)],
			requestedConnections: [
				{ agent: "mira", server: "linear", why: "I need it to track bugs." },
			],
			hiddenSuggestions: [{ agent: "mira", server: "linear" }],
		});
		expect(items()).toHaveLength(1);
		expect(items()[0]?.textContent).toContain("Notion");
		expect(screen.queryByText(/Linear/)).toBeNull();
	});

	it("does_not_suggest_what_is_held_at_launch_hidden_or_a_paused_agents", async () => {
		const held = {
			...mira,
			mcp_servers: [{ name: "linear", source: "kit" }],
		};
		await today({
			team: only(
				held,
				{
					...agent("kai", "Kai", "marketing_specialist", "marketing"),
					status: "paused",
				},
				{
					...agent("finn", "Finn", "finance_specialist", "finance"),
					status: "retired",
				},
			),
			kits: [
				pmKit(KIT_LINEAR, KIT_NOTION, KIT_ADS),
				{ role: "marketing_specialist", connectors: [KIT_NOTION] },
				{ role: "finance_specialist", connectors: [KIT_NOTION] },
			],
			hiddenSuggestions: [{ agent: "mira", server: "notion" }],
		});
		await screen.findByRole("heading", { name: /Waiting on you/ });
		expect(
			screen.queryByRole("heading", { name: en.suggestionsTitle }),
		).toBeNull();
	});

	it("not_now_hides_the_row_and_sends_the_command", async () => {
		const { s } = await today({
			team: only(mira),
			kits: [pmKit(KIT_NOTION, KIT_LINEAR)],
		});
		expect(items()).toHaveLength(2);
		fireEvent.click(
			within(items()[0] as HTMLElement).getByRole("button", {
				name: en.suggestionsNotNow,
			}),
		);
		expect(items()).toHaveLength(1);
		expect((items()[0] as HTMLElement).textContent).toContain("Linear");
		const sent = await waitFor(() => {
			const f = s.calls("connector_suggestion.hide")[0];
			if (!f) throw new Error("not sent");
			return f;
		});
		expect(sent.params).toEqual({ agent: "mira", server: "notion" });
		expect(refusedBy("connectorSuggestionHideRequest", sent.params)).toEqual(
			[],
		);
		await s.reply(sent, {});
		expect(items()).toHaveLength(1);
		expect((items()[0] as HTMLElement).textContent).toContain("Linear");
	});

	it("not_now_puts_the_row_back_when_refused", async () => {
		const { s } = await today({
			team: only(mira),
			kits: [pmKit(KIT_NOTION, KIT_LINEAR)],
		});
		const notNow = () =>
			fireEvent.click(
				screen.getAllByRole("button", {
					name: en.suggestionsNotNow,
				})[0] as HTMLElement,
			);
		notNow();
		const sent = await waitFor(() => {
			const f = s.calls("connector_suggestion.hide")[0];
			if (!f) throw new Error("not sent");
			return f;
		});
		await s.fail(
			sent,
			-32005,
			"suggestion_not_offered: mira is not offered notion",
		);
		expect(items()).toHaveLength(2);
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseCommand,
		);
		expect(document.body.textContent).not.toContain("suggestion_not_offered");
		notNow();
		await waitFor(() => {
			if (s.calls("connector_suggestion.hide").length < 2)
				throw new Error("not sent");
		});
		await s.reply(s.calls("connector_suggestion.hide")[1] as never, {});
		expect(screen.queryByRole("alert")).toBeNull();
	});

	it("not_now_keeps_focus_in_the_section", async () => {
		await today({
			team: only(mira),
			kits: [pmKit(KIT_NOTION, KIT_LINEAR)],
		});
		fireEvent.click(
			within(items()[0] as HTMLElement).getByRole("button", {
				name: en.suggestionsNotNow,
			}),
		);
		expect(document.activeElement).toBe(
			screen.getByRole("heading", { name: en.suggestionsTitle }),
		);
	});

	it("shows_four_then_all", async () => {
		await today({
			team: only(mira, ada, theo),
			kits: [
				pmKit(KIT_NOTION, KIT_LINEAR),
				{ role: "architect", connectors: [KIT_NOTION, KIT_LINEAR] },
				{ role: "software_developer", connectors: [KIT_NOTION, KIT_LINEAR] },
			],
		});
		expect(items()).toHaveLength(4);
		const all = screen.getByRole("button", { name: "Show all 6" });
		expect(all.getAttribute("aria-expanded")).toBe("false");
		expect(all.getAttribute("aria-controls")).toBe("suggestions-list");
		fireEvent.click(all);
		expect(items()).toHaveLength(6);
		const fewer = screen.getByRole("button", { name: en.suggestionsShowFewer });
		expect(fewer.getAttribute("aria-expanded")).toBe("true");
	});

	it("orders_by_team_then_kit", async () => {
		await today({
			team: only(mira, ada),
			kits: [
				{ role: "architect", connectors: [KIT_NOTION, KIT_LINEAR] },
				pmKit(KIT_LINEAR, KIT_NOTION),
			],
		});
		expect(
			items().map((row) => row.querySelector("strong")?.textContent),
		).toEqual(["Linear", "Notion", "Notion", "Linear"]);
		expect(items().map((row) => row.textContent?.includes("Mira"))).toEqual([
			true,
			true,
			false,
			false,
		]);
	});

	it("suggestions_have_no_accessibility_violations", async () => {
		const { container } = await today({
			team: only(mira),
			kits: [pmKit(KIT_NOTION, KIT_LINEAR)],
		});
		await screen.findByRole("heading", { name: en.suggestionsTitle });
		await expectNoAxeViolations(container);
	});
});
