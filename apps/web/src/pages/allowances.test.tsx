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
import { media } from "../test/media.ts";
import {
	answerQuery,
	answerStatus,
	eventArrives,
	renderApp,
} from "../test/render-app.tsx";

const ATTEMPT = "0123456789abcdef0123456789abcdef";
const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	persona: `${name} persona`,
	status: "active",
});
/** What the Marketing Specialist's kit offers: a service that signs in and spends credits. */
const HIGGSFIELD = {
	name: "higgsfield",
	title: "Higgsfield",
	about: "Higgsfield makes images, video and sound from a description.",
	why: "Makes pictures and short clips for launch posts, so a post never waits on artwork.",
	setup:
		"Sign in with your Higgsfield account. What Kai makes is paid for with your Higgsfield credits.",
	labels: {
		generate_image: "make an image",
		generate_video: "make a video",
		check_credits: "check your credits",
	},
	auth: "oauth",
	credential_keys: [],
	allowances: [
		{ tool: "generate_image", calls: 20, what: "images" },
		{ tool: "generate_video", calls: 5, what: "videos" },
	],
};
/** The same service, given keys instead of a sign-in. */
const HIGGSFIELD_KEYS = {
	...HIGGSFIELD,
	auth: "keys",
	key_page: "https://higgsfield.example/keys",
	credential_keys: ["HIGGSFIELD_KEY"],
};
const CANVA = {
	name: "canva",
	title: "Canva",
	about: "Canva holds your brand’s designs.",
	why: "Opens your brand’s designs, so new artwork matches what you already have.",
	setup: "Make a key on Canva’s page and paste it.",
	key_page: "https://canva.example/keys",
	labels: { find_design: "find a design", make_design: "make a design" },
	auth: "keys",
	credential_keys: ["CANVA_KEY"],
};
const entry = (allowances: Record<string, number>) => ({
	name: "higgsfield",
	source: "kit",
	transport: "http",
	url: "https://higgsfield.example/mcp",
	oauth: {},
	tools: {
		check_credits: "network",
		generate_image: "external_effect",
		generate_video: "external_effect",
	},
	allowances,
});
const team = (servers: object[] = []) => ({
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		{
			...agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
			...(servers.length > 0 && { mcp_servers: servers }),
		},
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
});
const EFFECTIVE = ["mira", "kai"].map((id) => ({
	id,
	model: { id: "claude-opus-5-5", label: "Strongest model", effort: "high" },
	tiers: ["read"],
	base_tiers: ["read"],
}));
const CONNECTED = [
	{
		agent: "kai",
		server: "higgsfield",
		state: "connected",
		auth: "oauth",
		source: "kit",
		revokes: true,
		stored_in: "keychain",
	},
];
const ROW = {
	agent: "kai",
	server: "higgsfield",
	what: "images",
	tool: "generate_image",
	used: 14,
	of: 20,
};
const VIDEOS = {
	agent: "kai",
	server: "higgsfield",
	what: "videos",
	tool: "generate_video",
	used: 2,
	of: 5,
};
const WIDE = "(min-width: 1024px)";
const sprintPeriod = { kind: "sprint", sprint_id: "S2" };

const teamGot = (
	servers: object[],
	services: object[],
	connectors: object[] = [],
) => ({
	team: team(servers),
	agents: EFFECTIVE,
	judges: { auto: null, architect: null, scrum_master: null },
	max_agents: 7,
	connectors,
	sandboxed: true,
	kits: [{ role: "marketing_specialist", connectors: services }],
});

/** Kai's page, the kit offering `services`, Kai holding `servers`, `rows` made so far. */
async function kaiPage(
	services: object[],
	servers: object[] = [],
	connectors: object[] = [],
	rows: object[] = [],
	period: object = sprintPeriod,
) {
	const { container, socket } = await renderApp("/team/kai");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", teamGot(servers, services, connectors));
	await answerQuery(s, "models.list", { models: [] });
	await answerQuery(s, "allowances.list", { period, rows });
	await screen.findByRole("heading", {
		name: "Kai, your Marketing Specialist",
	});
	return { container, s };
}

async function sent(socket: FakeSocket, method: string) {
	return waitFor(() => {
		const frame = socket.calls(method)[0];
		if (!frame) throw new Error(`no ${method} was sent`);
		return frame;
	});
}

const kitRow = (title: string) =>
	within(
		screen.getByRole("list", { name: "From the Marketing Specialist’s kit" }),
	)
		.getByText(title, { selector: "strong" })
		.closest("li") as HTMLElement;

/** Signing in done: the answer to the first poll is "signed in". */
async function signedIn(s: FakeSocket, dialog: HTMLElement) {
	const asked = await sent(s, "connector.sign_in");
	await s.reply(asked, {
		attempt: ATTEMPT,
		authorize_url: "https://higgsfield.example/authorize?state=abc",
		issuer: "https://higgsfield.example",
	});
	const button = await within(dialog).findByRole("button", {
		name: "Sign in with Higgsfield",
	});
	vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
	fireEvent.click(button);
	await act(() => vi.advanceTimersByTimeAsync(2100));
	const status = s.calls("connector.sign_in_status")[0] as NonNullable<
		ReturnType<FakeSocket["calls"]>[number]
	>;
	await s.reply(status, { state: "signed_in" });
	vi.useRealTimers();
}

describe("allowances on the screens", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.restoreAllMocks();
		vi.unstubAllGlobals();
	});

	it("connector_add_asks_how_many_for_a_kit_with_allowances", async () => {
		vi.spyOn(window, "open").mockReturnValue(null);
		const { container, s } = await kaiPage([HIGGSFIELD]);
		fireEvent.click(
			within(kitRow("Higgsfield")).getByRole("button", {
				name: "Connect Higgsfield",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Higgsfield to Kai",
		});
		await signedIn(s, dialog);
		// Signed in is not connected yet: the step asks how many, each starting at the kit's.
		expect(
			await within(dialog).findByText(
				"How many may Kai make each sprint without asking?",
			),
		).toBeTruthy();
		expect(s.calls("connector.connect")).toHaveLength(0);
		expect(within(dialog).getByText("Signed in to Higgsfield.")).toBeTruthy();
		const images = within(dialog).getByLabelText(/^Images/) as HTMLInputElement;
		const videos = within(dialog).getByLabelText(/^Videos/) as HTMLInputElement;
		expect([images.value, videos.value]).toEqual(["20", "5"]);
		expect(within(dialog).getByText("make an image")).toBeTruthy();
		expect(within(dialog).getByText("make a video")).toBeTruthy();
		expect(
			within(dialog).getByText(en.allowRange.replace("{name}", "Kai")),
		).toBeTruthy();
		await expectNoAxeViolations(container);
		fireEvent.change(images, { target: { value: "12" } });
		fireEvent.click(within(dialog).getByRole("button", { name: "Connect" }));
		const connect = await sent(s, "connector.connect");
		expect(connect.params).toEqual({
			agent: "kai",
			server: { name: "higgsfield", source: "kit" },
			attempt: ATTEMPT,
			allowances: { generate_image: 12, generate_video: 5 },
			tags: {},
		});
		await s.reply(connect, {
			stored_in: "keychain",
			tools: {
				check_credits: "network",
				generate_image: "external_effect",
				generate_video: "external_effect",
			},
		});
		// Done: the numbers are in what Kai can do, and what is beyond them asks.
		expect(
			await within(dialog).findByText("Higgsfield is connected to Kai"),
		).toBeTruthy();
		expect(
			within(dialog).getByText("make up to 12 images and 5 videos each sprint"),
		).toBeTruthy();
		expect(within(dialog).getByText("check your credits")).toBeTruthy();
		expect(within(dialog).getByText("making more than that")).toBeTruthy();
	});

	it("connector_add_asks_how_many_after_a_key_and_goes_back_with_it", async () => {
		const { s } = await kaiPage([HIGGSFIELD_KEYS]);
		fireEvent.click(
			within(kitRow("Higgsfield")).getByRole("button", {
				name: "Connect Higgsfield",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Higgsfield to Kai",
		});
		fireEvent.change(within(dialog).getByLabelText("Your Higgsfield key"), {
			target: { value: "hf-secret-1" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		const videos = (await within(dialog).findByLabelText(
			/^Videos/,
		)) as HTMLInputElement;
		expect(s.calls("connector.connect")).toHaveLength(0);
		// Back keeps the key; a zero is a number too.
		fireEvent.change(videos, { target: { value: "0" } });
		fireEvent.click(within(dialog).getByRole("button", { name: "Back" }));
		const key = within(dialog).getByLabelText(
			"Your Higgsfield key",
		) as HTMLInputElement;
		expect(key.value).toBe("hf-secret-1");
		fireEvent.click(within(dialog).getByRole("button", { name: "Next" }));
		fireEvent.change(await within(dialog).findByLabelText(/^Videos/), {
			target: { value: "0" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Connect" }));
		const connect = await sent(s, "connector.connect");
		expect(connect.params).toEqual({
			agent: "kai",
			server: { name: "higgsfield", source: "kit" },
			keys: { HIGGSFIELD_KEY: "hf-secret-1" },
			allowances: { generate_image: 20, generate_video: 0 },
			tags: {},
		});
		await s.reply(connect, {
			stored_in: "keychain",
			tools: {
				check_credits: "network",
				generate_image: "external_effect",
				generate_video: "external_effect",
			},
		});
		// A tool at 0 asks every time: it is not in what runs, and it is in what asks.
		expect(
			await within(dialog).findByText("make up to 20 images each sprint"),
		).toBeTruthy();
		const asks = within(dialog)
			.getByText("Kai asks you first before")
			.closest("div") as HTMLElement;
		expect(within(asks).getByText("make a video")).toBeTruthy();
		expect(within(asks).getByText("making more than that")).toBeTruthy();
	});

	it("connector_add_skips_the_step_without_allowances", async () => {
		const { s } = await kaiPage([CANVA]);
		fireEvent.click(
			within(kitRow("Canva")).getByRole("button", { name: "Connect Canva" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Canva to Kai",
		});
		expect(within(dialog).queryByText(en.allowStepHowMany)).toBeNull();
		fireEvent.change(within(dialog).getByLabelText("Your Canva key"), {
			target: { value: "canva-secret" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Connect" }));
		const connect = await sent(s, "connector.connect");
		expect(connect.params).toEqual({
			agent: "kai",
			server: { name: "canva", source: "kit" },
			keys: { CANVA_KEY: "canva-secret" },
			tags: {},
		});
	});

	it("connector_allowance_changes_the_number", async () => {
		const { container, s } = await kaiPage(
			[HIGGSFIELD, CANVA],
			[entry({ generate_image: 20, generate_video: 5 })],
			CONNECTED,
			[ROW, VIDEOS],
		);
		const higgsfield = kitRow("Higgsfield");
		expect(
			within(higgsfield).getByText(
				"This sprint: 14 of 20 images, 2 of 5 videos.",
			),
		).toBeTruthy();
		// Canva has no allowance, so no such button.
		expect(within(kitRow("Canva")).queryByText(/Change how many/)).toBeNull();
		fireEvent.click(
			within(higgsfield).getByRole("button", {
				name: "Change how many Kai may make with Higgsfield",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "How many may Kai make each sprint without asking?",
		});
		expect(within(dialog).getByText("14 made so far")).toBeTruthy();
		expect(within(dialog).getByText("2 made so far")).toBeTruthy();
		await expectNoAxeViolations(container);
		const images = within(dialog).getByLabelText(/^Images/) as HTMLInputElement;
		expect(images.value).toBe("20");
		// Over 1,000 is refused at its field, and nothing is sent.
		fireEvent.change(images, { target: { value: "1500" } });
		fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
		expect(
			await within(dialog).findByText("Up to 1,000. Type a smaller number."),
		).toBeTruthy();
		expect(s.calls("connector.allowances")).toHaveLength(0);
		fireEvent.change(images, { target: { value: "30" } });
		fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
		const change = await sent(s, "connector.allowances");
		expect(change.params).toEqual({
			agent: "kai",
			server: "higgsfield",
			allowances: { generate_image: 30, generate_video: 5 },
		});
		await s.reply(change, {});
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("connector_allowance_starts_at_the_entrys_number", async () => {
		await kaiPage(
			[HIGGSFIELD],
			[entry({ generate_image: 5, generate_video: 5 })],
			CONNECTED,
			[{ ...ROW, of: 5 }, VIDEOS],
		);
		fireEvent.click(
			within(kitRow("Higgsfield")).getByRole("button", {
				name: "Change how many Kai may make with Higgsfield",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "How many may Kai make each sprint without asking?",
		});
		expect(
			(within(dialog).getByLabelText(/^Images/) as HTMLInputElement).value,
		).toBe("5");
	});

	it("connector_allowance_says_connect_again_when_the_kit_changed", async () => {
		const { s } = await kaiPage(
			[HIGGSFIELD],
			[entry({ generate_image: 20, generate_video: 5 })],
			CONNECTED,
			[ROW, VIDEOS],
		);
		fireEvent.click(
			within(kitRow("Higgsfield")).getByRole("button", {
				name: "Change how many Kai may make with Higgsfield",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "How many may Kai make each sprint without asking?",
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
		const change = await sent(s, "connector.allowances");
		await s.fail(
			change,
			-32005,
			"connector_not_in_kit: higgsfield is not what the kit says it is now",
		);
		const again = await screen.findByRole("dialog", {
			name: "Connect Higgsfield again first",
		});
		expect(
			within(again).getByText(
				"Farik updated this service since you connected it. Connect it again to keep using it; you choose the numbers again there.",
			),
		).toBeTruthy();
		fireEvent.click(
			within(again).getByRole("button", { name: "Connect again" }),
		);
		expect(
			await screen.findByRole("dialog", { name: "Connect Higgsfield to Kai" }),
		).toBeTruthy();
	});

	it("agent_page_opens_the_dialog_from_the_approval_link", async () => {
		const { container, socket } = await renderApp(
			"/team/kai?allowances=higgsfield",
		);
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(
			s,
			"team.get",
			teamGot(
				[entry({ generate_image: 20, generate_video: 5 })],
				[HIGGSFIELD],
				CONNECTED,
			),
		);
		await answerQuery(s, "models.list", { models: [] });
		await answerQuery(s, "allowances.list", {
			period: sprintPeriod,
			rows: [ROW, VIDEOS],
		});
		expect(
			await screen.findByRole("dialog", {
				name: "How many may Kai make each sprint without asking?",
			}),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("board_shows_each_allowance_beside_the_sprint", async () => {
		const { container, socket } = await renderApp("/board");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", teamGot([], [HIGGSFIELD]));
		await answerQuery(s, "tasks.list", { tasks: [] });
		await answerQuery(s, "waiting.list", { waiting: [] });
		await answerQuery(s, "team.activity", { activity: [] });
		await answerQuery(s, "sprint.current", {
			sprint_id: "S2",
			done: 4,
			total: 7,
		});
		await answerQuery(s, "backlog.summary", {
			plan_in_sprints: false,
			count: 0,
		});
		await answerQuery(s, "sprints.list", { sprints: [] });
		let seq = 0;
		const list = async (rows: object[], period: object = sprintPeriod) => {
			// The page asks again after an event, and is answered with what is made by then.
			if (seq > 0) await eventArrives(s, seq);
			seq += 1;
			await answerQuery(s, "allowances.list", { period, rows });
		};
		await list([ROW, VIDEOS]);
		const region = await screen.findByRole("region", {
			name: "Made on other services",
		});
		const lines = () =>
			within(region)
				.getAllByRole("listitem")
				.map((item) => item.textContent);
		expect(lines()).toEqual([
			"Kai: 14 of 20 images this sprint",
			"Kai: 2 of 5 videos this sprint",
		]);
		await expectNoAxeViolations(container);
		// At the number, the line says Kai asks first.
		await list([{ ...ROW, used: 20 }, VIDEOS]);
		await waitFor(() =>
			expect(lines()[0]).toBe(
				"Kai: 20 of 20 images. Kai asks you before making more.",
			),
		);
		// Past it, the count shows as it is, and says why.
		await list([{ ...ROW, used: 21 }, VIDEOS]);
		await waitFor(() =>
			expect(lines()[0]).toBe(
				"Kai: 21 of 20 images. Kai asks you before making more. Extra images were ones you approved.",
			),
		);
		expect(lines()[1]).toBe("Kai: 2 of 5 videos this sprint");
		// With no sprint running, the count is the day's.
		await list([ROW], { kind: "day", day: "2026-10-02" });
		await waitFor(() =>
			expect(lines()).toEqual(["Kai: 14 of 20 images today"]),
		);
	});

	it("costs_lists_what_was_made_on_other_services", async () => {
		media.set(WIDE, true);
		const { container, socket } = await renderApp("/costs");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", teamGot([], [HIGGSFIELD]));
		await answerQuery(s, "costs.summary", {
			today_usd: 6.12,
			daily_limit_usd: null,
			sprint: {
				sprint_id: "S2",
				status: "open",
				spent_usd: 11.84,
				budget_usd: 20,
			},
			agents: [],
			conversations_today_usd: 0.42,
		});
		await answerQuery(s, "team.activity", { activity: [] });
		await answerQuery(s, "metrics", {
			accepted_tasks: 0,
			first_pass_acceptance_rate: null,
			interventions_per_accepted_task: null,
			cost_per_accepted_task: null,
			mechanically_verified_criteria_share: null,
			active_weeks: 0,
			messages: {
				reaction: 0,
				ambient: 0,
				reply: 0,
				ceremony: 0,
				system: 0,
				human: 0,
			},
		});
		await answerQuery(s, "allowances.list", {
			period: sprintPeriod,
			rows: [ROW, { ...VIDEOS, used: 6 }],
		});
		const section = await screen.findByRole("region", {
			name: "Made on other services",
		});
		const table = within(section).getByRole("table");
		const cells = within(table)
			.getAllByRole("row")
			.slice(1)
			.map((row) =>
				within(row)
					.getAllByRole("cell")
					.map((cell) => cell.textContent),
			);
		expect(
			within(table)
				.getAllByRole("columnheader")
				.map((head) => head.textContent),
		).toEqual(["Agent", "Service", "Made", "When"]);
		const names = within(table)
			.getAllByRole("row")
			.slice(1)
			.map((row) => within(row).getByRole("rowheader").textContent);
		expect(names).toEqual(["Kai", "Kai"]);
		expect(cells).toEqual([
			["Higgsfield", "14 of 20 images", "This sprint"],
			["Higgsfield", "6 of 5 videos", "This sprint"],
		]);
		const bill =
			"Farik counts what agents made, not what the service charges. Check your bill there.";
		expect(within(section).getByText(bill)).toBeTruthy();
		await expectNoAxeViolations(container);
		// On a phone each row stacks, as the board draws it.
		act(() => media.set(WIDE, false));
		expect(within(section).queryByRole("table")).toBeNull();
		expect(within(section).getAllByText("Kai, on Higgsfield")).toHaveLength(2);
		expect(within(section).getByText("14 of 20 images")).toBeTruthy();
		expect(within(section).getAllByText("This sprint")).toHaveLength(2);
		expect(within(section).getByText(bill)).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("costs_says_today_with_no_sprint_open", async () => {
		const { socket } = await renderApp("/costs");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", teamGot([], [HIGGSFIELD]));
		await answerQuery(s, "costs.summary", {
			today_usd: 0,
			daily_limit_usd: null,
			sprint: null,
			agents: [],
			conversations_today_usd: 0,
		});
		await answerQuery(s, "team.activity", { activity: [] });
		await answerQuery(s, "metrics", {
			accepted_tasks: 0,
			first_pass_acceptance_rate: null,
			interventions_per_accepted_task: null,
			cost_per_accepted_task: null,
			mechanically_verified_criteria_share: null,
			active_weeks: 0,
			messages: {
				reaction: 0,
				ambient: 0,
				reply: 0,
				ceremony: 0,
				system: 0,
				human: 0,
			},
		});
		await answerQuery(s, "allowances.list", {
			period: { kind: "day", day: "2026-09-22" },
			rows: [ROW],
		});
		const section = await screen.findByRole("region", {
			name: "Made on other services",
		});
		expect(within(section).getByText("Today")).toBeTruthy();
		expect(within(section).queryByText("This sprint")).toBeNull();
	});

	it("tool_approval_says_the_count_for_a_tool_with_an_allowance", async () => {
		const { container, socket } = await renderApp("/");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", teamGot([], [HIGGSFIELD]));
		await answerQuery(s, "team.activity", { activity: [] });
		await answerQuery(s, "allowances.list", {
			period: sprintPeriod,
			rows: [{ ...ROW, used: 20 }, VIDEOS],
		});
		await answerQuery(s, "waiting.list", {
			waiting: [
				{
					task_id: "FRK-16",
					kind: "tool_approval",
					agent_id: "kai",
					title: "Launch post for gift cards",
					line: "Kai wants to use higgsfield",
					approval: 41,
					server: "higgsfield",
					tool: "generate_image",
					input: '{"prompt":"A gift card"}',
				},
			],
		});
		fireEvent.click(
			await screen.findByRole("button", { name: en.waitingReview }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Kai wants to use Higgsfield",
		});
		expect(
			within(dialog).getByText(
				"Higgsfield, from the Marketing Specialist’s kit",
			),
		).toBeTruthy();
		expect(
			await within(dialog).findByText(
				"Kai has made 20 of 20 images this sprint.",
			),
		).toBeTruthy();
		const change = within(dialog).getByRole("link", {
			name: "Change how many",
		});
		expect(change.getAttribute("href")).toBe("/team/kai?allowances=higgsfield");
		expect(
			within(dialog).getByText(
				/A new number does not allow it; decide it here\.$/,
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("tool_approval_without_an_allowance_is_as_it_was", async () => {
		const { socket } = await renderApp("/");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", teamGot([], [HIGGSFIELD]));
		await answerQuery(s, "team.activity", { activity: [] });
		await answerQuery(s, "allowances.list", {
			period: sprintPeriod,
			rows: [ROW],
		});
		await answerQuery(s, "waiting.list", {
			waiting: [
				{
					task_id: "FRK-16",
					kind: "tool_approval",
					agent_id: "kai",
					title: "Launch post",
					line: "",
					approval: 42,
					server: "higgsfield",
					tool: "publish_site",
					input: "{}",
				},
			],
		});
		fireEvent.click(
			await screen.findByRole("button", { name: en.waitingReview }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Kai wants to use Higgsfield",
		});
		expect(within(dialog).queryByText(/made .* of/)).toBeNull();
		expect(
			within(dialog).queryByRole("link", { name: /Change how many/ }),
		).toBeNull();
	});

	it("tool_approval_names_a_server_outside_any_kit_as_it_is", async () => {
		const { socket } = await renderApp("/");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", teamGot([], [HIGGSFIELD]));
		await answerQuery(s, "team.activity", { activity: [] });
		await answerQuery(s, "allowances.list", { period: sprintPeriod, rows: [] });
		await answerQuery(s, "waiting.list", {
			waiting: [
				{
					task_id: "FRK-16",
					kind: "tool_approval",
					agent_id: "kai",
					title: "Launch post",
					line: "",
					approval: 43,
					server: "my-notes",
					tool: "save_note",
					input: "{}",
				},
			],
		});
		fireEvent.click(
			await screen.findByRole("button", { name: en.waitingReview }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Kai wants to use my-notes",
		});
		expect(
			within(dialog).getByText("my-notes, which you added to Kai"),
		).toBeTruthy();
	});

	it("allowance_screens_never_name_the_plumbing", () => {
		const plumbing = /\b(mcp|oauth|token)/i;
		const own = Object.entries(en).filter(([key]) => key.startsWith("allow"));
		expect(own.length).toBeGreaterThan(15);
		for (const [key, words] of own) expect(words, key).not.toMatch(plumbing);
		// The fixture kit's own copy, outside a label quoted from the service's page.
		for (const service of [HIGGSFIELD, HIGGSFIELD_KEYS]) {
			for (const field of ["title", "about", "why", "setup"] as const)
				expect(service[field], field).not.toMatch(plumbing);
			for (const allowance of service.allowances)
				expect(allowance.what).not.toMatch(plumbing);
			for (const label of Object.values(service.labels))
				expect(label).not.toMatch(plumbing);
		}
	});
});
