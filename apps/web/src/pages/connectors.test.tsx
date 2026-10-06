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
import { t } from "../strings/t.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	persona: `${name} persona`,
	status: "active",
});
/** Theo's custom connectors, as `team.yaml` holds them (ADR 0030). */
const AIRTABLE = {
	name: "airtable",
	source: "custom",
	transport: "stdio",
	command: "npx",
	args: ["-y", "airtable-connector"],
	credential_keys: ["AIRTABLE_API_KEY"],
	tools: {
		list_bases: "network",
		list_records: "network",
		create_record: "external_effect",
		delete_records: "denied",
	},
};
const LINEAR = {
	name: "linear",
	source: "custom",
	transport: "http",
	url: "https://connect.linear.example/v1",
	headers: {
		Authorization: "Bearer {LINEAR_KEY}",
		"X-Workspace": "corner-bakery",
	},
	credential_keys: ["LINEAR_KEY"],
	tools: { list_issues: "network" },
};
const NOTION = {
	name: "notion",
	source: "custom",
	transport: "http",
	url: "https://connect.notion.example/v1",
	credential_keys: [],
	tools: { search: "network" },
};
const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		{
			...agent("theo", "Theo", "software_developer", "developer"),
			mcp_servers: [AIRTABLE, LINEAR, NOTION],
		},
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};
const EFFECTIVE = ["mira", "theo"].map((id) => ({
	id,
	model: { id: "claude-opus-5-5", label: "Strongest model", effort: "high" },
	tiers: ["read"],
	base_tiers: ["read"],
}));
const CONNECTORS = [
	{ agent: "theo", server: "airtable", state: "connected" },
	{ agent: "theo", server: "linear", state: "connect_again" },
	{ agent: "theo", server: "notion", state: "store_unavailable" },
];

/** Theo's page, with the team and its connectors answered, sessions in the sandbox or not. */
async function opened(connectors: object[] = CONNECTORS, sandboxed = true) {
	const { container, socket } = await renderApp("/team/theo");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", {
		team: TEAM,
		agents: EFFECTIVE,
		judges: { auto: null, architect: null, scrum_master: null },
		max_agents: 7,
		connectors,
		sandboxed,
	});
	await answerQuery(s, "models.list", { models: [] });
	await screen.findByRole("heading", { name: "Theo, your Developer" });
	return { container, s };
}

/** The latest request of `method` the page sent. */
async function sent(socket: FakeSocket, method: string, count = 1) {
	return waitFor(() => {
		const all = socket.calls(method);
		const frame = all[count - 1];
		if (!frame) throw new Error(`no ${method} #${count} was sent`);
		return frame;
	});
}

const teamAskedTimes = (s: FakeSocket) =>
	s.calls("query").filter((q) => q.params.name === "team.get").length;

/** A row of "Added by you", by its server's name. */
const row = (name: string) => {
	const yours = screen.getByRole("list", { name: en.connectorsYours });
	return within(yours)
		.getByText(name, { selector: "strong" })
		.closest("li") as HTMLElement;
};

/** ConnectorAdd, opened from Advanced, step 1 filled in for airtable with `key`. */
async function startedAdding(key = "pat-secret-1", sandboxed = true) {
	const page = await opened([], sandboxed);
	fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
	fireEvent.click(screen.getByRole("button", { name: en.connectorCustomAdd }));
	const dialog = await screen.findByRole("dialog", {
		name: "Add a custom connector to Theo",
	});
	const field = (label: string) => within(dialog).getByLabelText(label);
	fireEvent.change(field(en.addName), { target: { value: "airtable" } });
	fireEvent.change(field(en.addCommand), { target: { value: "npx" } });
	for (const [i, part] of ["-y", "airtable-connector"].entries()) {
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.addArgMore }),
		);
		fireEvent.change(field(`Part ${i + 1}`), { target: { value: part } });
	}
	fireEvent.change(field(en.addKeyName), {
		target: { value: "AIRTABLE_API_KEY" },
	});
	fireEvent.change(field(en.addKeyValue), { target: { value: key } });
	return { ...page, dialog };
}

/** Steps 1 and 2 done: the tools listed, the user's labels left as they come. */
async function listed(key = "pat-secret-1", sandboxed = true) {
	const page = await startedAdding(key, sandboxed);
	fireEvent.click(
		within(page.dialog).getByRole("button", { name: en.addNext }),
	);
	const asked = await sent(page.s, "connector.tools");
	await page.s.reply(asked, {
		tools: [
			{ name: "list_bases", description: "List the bases.", usable: true },
			{ name: "delete_records", description: "Delete records.", usable: true },
			{ name: "records.export", description: "Export a table.", usable: false },
		],
	});
	await within(page.dialog).findByRole("group", { name: "list_bases" });
	return { ...page, asked };
}

/** The team.get answer for `team`, as the daemon gives it after a connect or a disconnect. */
const teamGot = (team: object, connectors: object[] = CONNECTORS) => ({
	team,
	agents: EFFECTIVE,
	judges: { auto: null, architect: null, scrum_master: null },
	max_agents: 7,
	connectors,
	sandboxed: true,
});

/** Theo's team with `servers` in place of Theo's custom connectors. */
const theoWith = (servers: object[]) => ({
	...TEAM,
	agents: [
		TEAM.agents[0],
		{
			...agent("theo", "Theo", "software_developer", "developer"),
			mcp_servers: servers,
		},
	],
});

/** The persona typed, so the page holds a draft of Theo. */
function editPersona() {
	fireEvent.change(screen.getByLabelText("How Theo talks"), {
		target: { value: "Short and kind." },
	});
}

/** Save pressed: Theo's entry in the team `team.save` sent. */
async function savedTheo(s: FakeSocket) {
	fireEvent.click(screen.getByRole("button", { name: en.agentSave }));
	const save = await sent(s, "team.save");
	const team = (
		save.params as {
			team: {
				agents: { persona?: string; mcp_servers?: { name: string }[] }[];
			};
		}
	).team;
	return team.agents[1];
}

describe("connectors on the agent page", () => {
	afterEach(() => localStorage.clear());

	it("agent_edit_lists_connectors_and_removes_after_confirming", async () => {
		const { container, s } = await opened();
		const airtable = row("airtable");
		expect(within(airtable).getByText(en.connectorStdio)).toBeTruthy();
		expect(
			within(airtable).getByText(
				"4 tools: 2 Only reads, 1 Changes things, asks you, 1 Never",
			),
		).toBeTruthy();
		expect(within(row("linear")).getByText(en.connectorHttp)).toBeTruthy();
		await expectNoAxeViolations(container);

		// Remove asks first, and Keep it sends nothing.
		fireEvent.click(
			within(airtable).getByRole("button", { name: "Remove airtable" }),
		);
		let dialog = await screen.findByRole("dialog", {
			name: "Remove airtable from Theo?",
		});
		await expectNoAxeViolations(container);
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.connectorKeep }),
		);
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("connector.disconnect")).toHaveLength(0);

		fireEvent.click(
			within(row("airtable")).getByRole("button", { name: "Remove airtable" }),
		);
		dialog = await screen.findByRole("dialog", {
			name: "Remove airtable from Theo?",
		});
		const before = teamAskedTimes(s);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Remove airtable" }),
		);
		const gone = await sent(s, "connector.disconnect");
		expect(gone.params).toEqual({ agent: "theo", server: "airtable" });
		await s.reply(gone, {});
		// The page reads the team again rather than guessing what is left.
		await waitFor(() => expect(teamAskedTimes(s)).toBeGreaterThan(before));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("agent_edit_saves_without_bringing_back_a_removed_connector", async () => {
		const { container, s } = await opened();
		editPersona();
		fireEvent.click(
			within(row("airtable")).getByRole("button", { name: "Remove airtable" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Remove airtable from Theo?",
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Remove airtable" }),
		);
		await s.reply(await sent(s, "connector.disconnect"), {});
		await answerQuery(s, "team.get", teamGot(theoWith([LINEAR, NOTION])));
		await waitFor(() =>
			expect(screen.queryByText("airtable", { selector: "strong" })).toBeNull(),
		);
		await expectNoAxeViolations(container);
		const theo = await savedTheo(s);
		expect(theo?.persona).toBe("Short and kind.");
		expect(theo?.mcp_servers?.map((c) => c.name)).toEqual(["linear", "notion"]);
	});

	it("agent_edit_saves_without_dropping_an_added_connector", async () => {
		const { container, s } = await opened();
		editPersona();
		const github = {
			...AIRTABLE,
			name: "github",
			args: ["-y", "github-connector"],
		};
		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		fireEvent.click(
			screen.getByRole("button", { name: en.connectorCustomAdd }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add a custom connector to Theo",
		});
		const field = (label: string) => within(dialog).getByLabelText(label);
		fireEvent.change(field(en.addName), { target: { value: "github" } });
		fireEvent.change(field(en.addCommand), {
			target: { value: "github-connector" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		await s.reply(await sent(s, "connector.tools"), {
			tools: [{ name: "search", description: "", usable: true }],
		});
		fireEvent.click(
			await within(dialog).findByRole("button", { name: "Add github to Theo" }),
		);
		await s.reply(await sent(s, "connector.connect"), {
			stored_in: "keychain",
			tools: { search: "external_effect" },
		});
		const before = teamAskedTimes(s);
		fireEvent.click(
			await within(dialog).findByRole("button", { name: "Back to Theo" }),
		);
		// The page reads the team again once ConnectorAdd closes.
		await waitFor(() => expect(teamAskedTimes(s)).toBeGreaterThan(before));
		await answerQuery(
			s,
			"team.get",
			teamGot(theoWith([AIRTABLE, LINEAR, NOTION, github]), [
				...CONNECTORS,
				{
					agent: "theo",
					server: "github",
					state: "connected",
					stored_in: "keychain",
				},
			]),
		);
		await waitFor(() => expect(row("github")).toBeTruthy());
		await expectNoAxeViolations(container);
		const theo = await savedTheo(s);
		expect(theo?.persona).toBe("Short and kind.");
		expect(theo?.mcp_servers?.map((c) => c.name)).toEqual([
			"airtable",
			"linear",
			"notion",
			"github",
		]);
	});

	it("agent_edit_says_where_each_connectors_keys_are_kept", async () => {
		const { container } = await opened([
			{
				agent: "theo",
				server: "airtable",
				state: "connected",
				stored_in: "keychain",
			},
			{
				agent: "theo",
				server: "linear",
				state: "connect_again",
				stored_in: "file",
			},
			{ agent: "theo", server: "notion", state: "store_unavailable" },
		]);
		const keychain = "Theo’s keys are in your keychain.";
		const file = "Theo’s keys are in a private file on this computer.";
		expect(within(row("airtable")).getByText(keychain)).toBeTruthy();
		expect(within(row("linear")).getByText(file)).toBeTruthy();
		// Nothing read, nothing said.
		expect(within(row("notion")).queryByText(/keys are in/)).toBeNull();
		await expectNoAxeViolations(container);

		// Remove says which store it deletes the keys from.
		for (const [server, from] of [
			["airtable", "from your keychain."],
			["linear", "from the private file on this computer."],
			["notion", "from this computer."],
		] as const) {
			fireEvent.click(
				within(row(server)).getByRole("button", { name: `Remove ${server}` }),
			);
			const dialog = await screen.findByRole("dialog", {
				name: `Remove ${server} from Theo?`,
			});
			expect(
				within(dialog).getByText(
					`Theo stops using ${server} from the next piece of work. Farik deletes the keys you gave it for Theo ${from}`,
				),
			).toBeTruthy();
			await expectNoAxeViolations(container);
			fireEvent.click(
				within(dialog).getByRole("button", { name: en.connectorKeep }),
			);
			await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		}
	});

	it("agent_edit_advanced_says_where_a_command_runs", async () => {
		const { container } = await opened();
		expect(screen.queryByText(/More connectors arrive/)).toBeNull();
		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		expect(
			screen.getByText(
				/It runs in a folder Farik keeps for it, so give a file of yours by its full path, starting with \/\./,
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("agent_edit_says_a_refused_remove_in_plain_words", async () => {
		const { container, s } = await opened();
		fireEvent.click(
			within(row("airtable")).getByRole("button", { name: "Remove airtable" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Remove airtable from Theo?",
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Remove airtable" }),
		);
		const gone = await sent(s, "connector.disconnect");
		await s.fail(gone, -32005, "keyring: platform secure storage failure");
		expect(await within(dialog).findByText(en.refuseOther)).toBeTruthy();
		expect(within(dialog).queryByText(/keyring/)).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("agent_edit_shows_connect_again", async () => {
		const { container } = await opened();
		const linear = row("linear");
		expect(within(linear).getByText(en.connectorAgain)).toBeTruthy();
		expect(within(row("airtable")).queryByText(en.connectorAgain)).toBeNull();
		// A keychain that could not be read is said as that, not as a change.
		const notion = row("notion");
		expect(
			within(notion).getByText(
				"Farik cannot read Theo’s keys for notion right now",
			),
		).toBeTruthy();
		expect(within(notion).queryByText(en.connectorAgain)).toBeNull();
		// Only a changed connector offers Connect again.
		for (const steady of [row("airtable"), notion])
			expect(
				within(steady).queryByRole("button", { name: en.connectorAgainButton }),
			).toBeNull();
		await expectNoAxeViolations(container);

		fireEvent.click(
			within(linear).getByRole("button", { name: en.connectorAgainButton }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add linear to Theo",
		});
		const value = (label: string) =>
			(within(dialog).getByLabelText(label) as HTMLInputElement).value;
		expect(value(en.addName)).toBe("linear");
		expect(value(en.addUrl)).toBe("https://connect.linear.example/v1");
		expect(value(en.addHeader)).toBe("Authorization: Bearer {LINEAR_KEY}");
		expect(value(en.addKeyName)).toBe("LINEAR_KEY");
		// The key is typed again; the old one is never read back.
		expect(value(en.addKeyValue)).toBe("");
		expect(
			(within(dialog).getByLabelText(en.addKeyValue) as HTMLInputElement).type,
		).toBe("password");
		expect(
			within(dialog).getByText(
				/linear’s settings in your project changed since you connected it/,
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("connect_again_keeps_every_header", async () => {
		const { container, s } = await opened();
		fireEvent.click(
			within(row("linear")).getByRole("button", {
				name: en.connectorAgainButton,
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add linear to Theo",
		});
		await expectNoAxeViolations(container);
		fireEvent.change(within(dialog).getByLabelText(en.addKeyValue), {
			target: { value: "lin-key" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		const asked = await sent(s, "connector.tools");
		expect(
			(asked.params as { server: { headers: object } }).server.headers,
		).toEqual({
			Authorization: "Bearer {LINEAR_KEY}",
			"X-Workspace": "corner-bakery",
		});
	});

	it("connector_add_takes_each_part_of_a_command_in_its_own_field", async () => {
		// Split on spaces, a path holding one could not be entered (re-review N6).
		const { container, s } = await opened([]);
		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		fireEvent.click(
			screen.getByRole("button", { name: en.connectorCustomAdd }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add a custom connector to Theo",
		});
		const field = (label: string) => within(dialog).getByLabelText(label);
		fireEvent.change(field(en.addName), { target: { value: "files" } });
		fireEvent.change(field(en.addCommand), { target: { value: "node" } });
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.addArgMore }),
		);
		fireEvent.change(field("Part 1"), {
			target: { value: "/home/u/My Servers/x.js" },
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.addArgMore }),
		);
		fireEvent.change(field("Part 2"), { target: { value: "--read-only" } });
		fireEvent.change(field(en.addKeyName), { target: { value: "FILES_KEY" } });
		await expectNoAxeViolations(container);
		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		const asked = await sent(s, "connector.tools");
		const server = (asked.params as { server: object }).server;
		expect(server).toMatchObject({
			command: "node",
			args: ["/home/u/My Servers/x.js", "--read-only"],
		});

		// A refusal at one part is said at that part, in plain words.
		await s.fail(asked, -32005, "the team file is not valid", {
			errors: [
				{
					path: "/agents/1/mcp_servers/0/args/1",
					message: "arg_holds_secret: argument 2 looks like a key",
					code: "invalid",
				},
			],
		});
		expect(field("Part 2").getAttribute("aria-invalid")).toBe("true");
		expect(field("Part 1").getAttribute("aria-invalid")).not.toBe("true");
		expect(within(dialog).getByText(en.addArgSecret)).toBeTruthy();
		expect(within(dialog).queryByText(/arg_holds_secret/)).toBeNull();
		await expectNoAxeViolations(container);

		// A part is removed by its own button.
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Remove part 2" }),
		);
		expect(within(dialog).queryByLabelText("Part 2")).toBeNull();
	});

	it("connector_add_says_early_when_a_whole_command_is_pasted_into_command", async () => {
		// Re-review 2 m5: "npx -y foo" in Command was sent, and failed in general words.
		await opened([]);
		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		fireEvent.click(
			screen.getByRole("button", { name: en.connectorCustomAdd }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add a custom connector to Theo",
		});
		const field = (label: string) => within(dialog).getByLabelText(label);
		const next = within(dialog).getByRole("button", { name: en.addNext });
		fireEvent.change(field(en.addName), { target: { value: "files" } });
		fireEvent.change(field(en.addCommand), {
			target: { value: "npx -y @x/srv" },
		});
		expect(within(dialog).getByText(en.addCommandWhole)).toBeTruthy();
		expect(field(en.addCommand).getAttribute("aria-invalid")).toBe("true");
		expect(next.hasAttribute("disabled")).toBe(true);
		// A full path may hold a space.
		fireEvent.change(field(en.addCommand), {
			target: { value: "/home/u/My Servers/srv" },
		});
		expect(within(dialog).queryByText(en.addCommandWhole)).toBeNull();
		expect(next.hasAttribute("disabled")).toBe(false);
	});

	it("connector_add_says_a_refusal_at_the_part_it_names_after_an_empty_one", async () => {
		// Re-review 2 m5: an empty part is not sent, so the daemon counts the parts after it
		// one lower.
		const { s } = await opened([]);
		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		fireEvent.click(
			screen.getByRole("button", { name: en.connectorCustomAdd }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add a custom connector to Theo",
		});
		const field = (label: string) => within(dialog).getByLabelText(label);
		fireEvent.change(field(en.addName), { target: { value: "files" } });
		fireEvent.change(field(en.addCommand), { target: { value: "srv" } });
		const more = within(dialog).getByRole("button", { name: en.addArgMore });
		fireEvent.click(more);
		fireEvent.click(more);
		fireEvent.change(field("Part 2"), { target: { value: "sk-abc" } });
		fireEvent.change(field(en.addKeyName), { target: { value: "FILES_KEY" } });
		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		const asked = await sent(s, "connector.tools");
		expect(
			(asked.params as { server: { args: string[] } }).server.args,
		).toEqual(["sk-abc"]);
		await s.fail(asked, -32005, "the team file is not valid", {
			errors: [
				{
					path: "/agents/1/mcp_servers/0/args/0",
					message: "arg_holds_secret: argument 1 looks like a key",
					code: "invalid",
				},
			],
		});
		expect(field("Part 2").getAttribute("aria-invalid")).toBe("true");
		expect(field("Part 1").getAttribute("aria-invalid")).not.toBe("true");
	});

	it("connector_add_says_when_farik_settings_folder_is_inside_the_project", async () => {
		// Re-review 2 m1: no stdio server starts among the project's files, and the page says why.
		const { s } = await opened([]);
		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		fireEvent.click(
			screen.getByRole("button", { name: en.connectorCustomAdd }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add a custom connector to Theo",
		});
		const field = (label: string) => within(dialog).getByLabelText(label);
		fireEvent.change(field(en.addName), { target: { value: "files" } });
		fireEvent.change(field(en.addCommand), { target: { value: "srv" } });
		fireEvent.change(field(en.addKeyName), { target: { value: "FILES_KEY" } });
		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		const asked = await sent(s, "connector.tools");
		await s.fail(
			asked,
			-32005,
			"state_inside_project: Farik's settings folder, /home/u/app/.cfg/farik, is inside this project",
		);
		expect(
			await within(dialog).findByText(en.addStateInsideProject),
		).toBeTruthy();
	});

	it("connect_again_shows_each_part_of_the_command", async () => {
		await opened([
			{ agent: "theo", server: "airtable", state: "connect_again" },
		]);
		fireEvent.click(
			within(row("airtable")).getByRole("button", {
				name: en.connectorAgainButton,
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add airtable to Theo",
		});
		const value = (label: string) =>
			(within(dialog).getByLabelText(label) as HTMLInputElement).value;
		expect(value(en.addCommand)).toBe("npx");
		expect(value("Part 1")).toBe("-y");
		expect(value("Part 2")).toBe("airtable-connector");
	});

	it("connect_again_shows_every_header_and_lets_one_go", async () => {
		// Every header after the first was sent unseen, and could not be taken out (re-review N7).
		const { container, s } = await opened();
		fireEvent.click(
			within(row("linear")).getByRole("button", {
				name: en.connectorAgainButton,
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add linear to Theo",
		});
		const field = (label: string) =>
			within(dialog).getByLabelText(label) as HTMLInputElement;
		expect(field(en.addHeader).value).toBe(
			"Authorization: Bearer {LINEAR_KEY}",
		);
		expect(field("Header 2").value).toBe("X-Workspace: corner-bakery");
		await expectNoAxeViolations(container);

		// Named as another, it is said there, rather than one silently replacing the other.
		fireEvent.change(field(en.addHeader), {
			target: { value: "X-Workspace: Bearer {LINEAR_KEY}" },
		});
		expect(field("Header 2").getAttribute("aria-invalid")).toBe("true");
		expect(within(dialog).getByText(en.addHeaderTwice)).toBeTruthy();
		fireEvent.change(field(en.addHeader), {
			target: { value: "Authorization: Bearer {LINEAR_KEY}" },
		});

		fireEvent.click(
			within(dialog).getByRole("button", { name: "Remove header 2" }),
		);
		expect(within(dialog).queryByLabelText("Header 2")).toBeNull();
		fireEvent.change(field(en.addKeyValue), { target: { value: "lin-key" } });
		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		const asked = await sent(s, "connector.tools");
		expect(
			(asked.params as { server: { headers: object } }).server.headers,
		).toEqual({ Authorization: "Bearer {LINEAR_KEY}" });
	});

	it("connector_add_offers_three_labels_and_defaults_to_asks", async () => {
		const { container, s, dialog, asked } = await listed();
		expect(asked.params).toEqual({
			agent: "theo",
			server: {
				name: "airtable",
				transport: "stdio",
				command: "npx",
				args: ["-y", "airtable-connector"],
				credential_keys: ["AIRTABLE_API_KEY"],
			},
			keys: { AIRTABLE_API_KEY: "pat-secret-1" },
		});
		const bases = within(dialog).getByRole("group", { name: "list_bases" });
		const radios = within(bases).getAllByRole("radio") as HTMLInputElement[];
		expect(radios.map((r) => r.labels?.[0]?.textContent)).toEqual([
			en.tagNetwork,
			en.tagExternal,
			en.tagDenied,
		]);
		expect(
			radios.filter((r) => r.checked).map((r) => r.labels?.[0]?.textContent),
		).toEqual([en.tagExternal]);
		// A tool Farik cannot pass on is shown, never offered a label.
		expect(
			within(dialog).queryByRole("group", { name: "records.export" }),
		).toBeNull();
		expect(within(dialog).getByText("records.export")).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(within(bases).getByLabelText(en.tagNetwork));
		const deletes = within(dialog).getByRole("group", {
			name: "delete_records",
		});
		fireEvent.click(within(deletes).getByLabelText(en.tagDenied));
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Add airtable to Theo" }),
		);
		const connect = await sent(s, "connector.connect");
		expect(connect.params).toEqual({
			...asked.params,
			tags: { list_bases: "network", delete_records: "denied" },
		});
	});

	it("connector_add_sends_an_untouched_tool_as_asks", async () => {
		// A label the user never chose is "Changes things, asks you", on the wire too (W2).
		const { container, s, dialog } = await listed();
		await expectNoAxeViolations(container);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Add airtable to Theo" }),
		);
		const connect = await sent(s, "connector.connect");
		expect((connect.params as { tags: object }).tags).toEqual({
			list_bases: "external_effect",
			delete_records: "external_effect",
		});
	});

	it("connector_add_clears_the_key_field_after_sending", async () => {
		const { container, s, dialog } = await listed("pat-secret-1");
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Add airtable to Theo" }),
		);
		const connect = await sent(s, "connector.connect");
		await s.fail(
			connect,
			-32005,
			"the server did not answer within thirty seconds",
		);
		// Back on step 1, the key is to be typed again: nothing kept it.
		const key = (await within(dialog).findByLabelText(
			en.addKeyValue,
		)) as HTMLInputElement;
		expect(key.value).toBe("");
		expect(
			within(dialog).getByText(
				"airtable did not answer within thirty seconds. Check the command or the address, and try again.",
			),
		).toBeTruthy();
		expect(container.innerHTML).not.toContain("pat-secret-1");
		await expectNoAxeViolations(container);

		// Typed again and sent, it is gone from the page and from the browser's storage.
		fireEvent.change(key, { target: { value: "pat-secret-2" } });
		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		const again = await sent(s, "connector.tools", 2);
		await s.reply(again, {
			tools: [{ name: "list_bases", description: "", usable: true }],
		});
		fireEvent.click(
			await within(dialog).findByRole("button", {
				name: "Add airtable to Theo",
			}),
		);
		const second = await sent(s, "connector.connect", 2);
		expect((second.params as { keys: object }).keys).toEqual({
			AIRTABLE_API_KEY: "pat-secret-2",
		});
		await s.reply(second, {
			stored_in: "keychain",
			tools: { list_bases: "external_effect" },
		});
		await within(dialog).findByText("airtable is added to Theo");
		expect(container.innerHTML).not.toContain("pat-secret-2");
		const stored = [localStorage, sessionStorage].flatMap((store) =>
			Object.keys(store).map((k) => store.getItem(k) ?? ""),
		);
		expect(stored.join(" ")).not.toContain("pat-secret");
	});

	it("connector_add_says_a_refusal_at_its_field", async () => {
		const { container, s, dialog } = await startedAdding();
		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		const asked = await sent(s, "connector.tools");
		await s.fail(asked, -32005, "the team file is not valid", {
			errors: [
				{
					path: "/agents/1/mcp_servers/0/name",
					message: "connector_name_reserved: farik is a name Farik keeps",
					code: "invalid",
				},
				{
					path: "/agents/1/mcp_servers/0/credential_keys/0",
					message: "does not match",
					code: "invalid",
				},
			],
		});
		const name = within(dialog).getByLabelText(en.addName);
		expect(name.getAttribute("aria-invalid")).toBe("true");
		expect(within(dialog).getByText(en.addNameReserved)).toBeTruthy();
		expect(within(dialog).getByText(en.addKeyWrong)).toBeTruthy();
		expect(
			within(dialog).queryByText(/does not match|is a name Farik keeps/),
		).toBeNull();
		await expectNoAxeViolations(container);

		// A web address holding a key is refused before anything is sent.
		fireEvent.click(
			within(dialog).getByRole("radio", { name: new RegExp(en.addUrlChoice) }),
		);
		fireEvent.change(within(dialog).getByLabelText(en.addUrl), {
			target: { value: "https://actions.zapier.example/sk-ak/connect?key=v" },
		});
		expect(within(dialog).getByText(en.addUrlSecret)).toBeTruthy();
		const next = within(dialog).getByRole("button", {
			name: en.addNext,
		}) as HTMLButtonElement;
		expect(next.disabled).toBe(true);
	});

	it("connector_add_words_the_newer_refusals_at_their_fields", async () => {
		const { container, s, dialog } = await startedAdding();
		const field = (label: string) => within(dialog).getByLabelText(label);
		const next = () =>
			fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		next();
		await s.fail(
			await sent(s, "connector.tools"),
			-32005,
			"the team file is not valid",
			{
				errors: [
					{
						path: "/agents/1/mcp_servers/0/command",
						message:
							"command_not_absolute: bin/mcp would be looked for in the folder",
						code: "invalid",
					},
				],
			},
		);
		expect(field(en.addCommand).getAttribute("aria-invalid")).toBe("true");
		expect(within(dialog).getByText(en.addCommandNotAbsolute)).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(
			within(dialog).getByRole("radio", { name: new RegExp(en.addUrlChoice) }),
		);
		fireEvent.change(field(en.addUrl), {
			target: { value: "https://connect.airtable.example/v1" },
		});
		fireEvent.change(field(en.addHeader), {
			target: { value: "Authorization: Bearer pat-typed-here" },
		});
		next();
		await s.fail(
			await sent(s, "connector.tools", 2),
			-32005,
			"the team file is not valid",
			{
				errors: [
					{
						path: "/agents/1/mcp_servers/0/headers/Authorization",
						message:
							"header_holds_secret: Authorization holds its value itself",
						code: "invalid",
					},
				],
			},
		);
		expect(field(en.addHeader).getAttribute("aria-invalid")).toBe("true");
		expect(within(dialog).getByText(en.addHeaderSecret)).toBeTruthy();
		await expectNoAxeViolations(container);
		expect(
			within(dialog).queryByText(
				/command_not_absolute|header_holds_secret|bin\/mcp/,
			),
		).toBeNull();
	});

	it("connector_add_says_a_tool_list_that_changed", async () => {
		const { container, s, dialog } = await listed();
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Add airtable to Theo" }),
		);
		await s.fail(
			await sent(s, "connector.connect"),
			-32005,
			"tag_unknown_tool: delete_records is not a tool this server lists that Farik can use; its tools are list_bases",
		);
		expect(
			await within(dialog).findByText(
				"airtable’s tools changed since Farik listed them. Press “Next: list its tools” to list them again, then label each one.",
			),
		).toBeTruthy();
		expect(within(dialog).queryByText(/tag_unknown_tool/)).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("connector_add_promises_theo_never_sees_the_keys_only_in_the_sandbox", async () => {
		// Without the sandbox a command Theo runs can read them (spec 8.6, re-review N5).
		for (const [sandboxed, storedIn] of [
			[true, "keychain"],
			[true, "file"],
			[false, "keychain"],
			[false, "file"],
		] as const) {
			const { container, s, dialog } = await listed("pat-secret-1", sandboxed);
			fireEvent.click(
				within(dialog).getByRole("button", { name: "Add airtable to Theo" }),
			);
			await s.reply(await sent(s, "connector.connect"), {
				stored_in: storedIn,
				tools: { list_bases: "network" },
			});
			await within(dialog).findByText("airtable is added to Theo");
			const said = dialog.textContent ?? "";
			expect(
				said.includes("Theo never sees them"),
				`${sandboxed} ${storedIn}`,
			).toBe(sandboxed);
			if (!sandboxed)
				expect(said).toContain(
					"Without Docker’s sandbox, a command Theo runs on this computer could reach them.",
				);
			await expectNoAxeViolations(container);
			cleanup();
			localStorage.clear();
		}
	});

	it("connector_add_says_which_store_kept_the_keys", async () => {
		for (const [storedIn, words] of [
			[
				"keychain",
				"Theo’s keys for airtable are kept in your computer’s keychain.",
			],
			[
				"file",
				"Theo’s keys for airtable are kept in a private file only you can read.",
			],
		] as const) {
			const { container, s, dialog } = await listed();
			fireEvent.click(
				within(dialog).getByRole("button", { name: "Add airtable to Theo" }),
			);
			const connect = await sent(s, "connector.connect");
			await s.reply(connect, {
				stored_in: storedIn,
				tools: {
					list_bases: "network",
					delete_records: "external_effect",
				},
			});
			expect(await within(dialog).findByText(words)).toBeTruthy();
			expect(
				within(dialog).getByText(
					"1 Only reads, 1 Changes things, asks you, 1 Farik can’t use",
				),
			).toBeTruthy();
			await expectNoAxeViolations(container);
			const before = teamAskedTimes(s);
			fireEvent.click(
				within(dialog).getByRole("button", { name: "Back to Theo" }),
			);
			await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
			expect(teamAskedTimes(s)).toBeGreaterThan(before);
			cleanup();
			localStorage.clear();
		}
	});
});

/** The tests of signing in to a service (ADR 0033). */
const ATTEMPT = "0123456789abcdef0123456789abcdef";
const SIGNED_NOTION = {
	name: "notion",
	source: "custom",
	transport: "http",
	url: "https://mcp.notion.com/mcp",
	oauth: {},
	tools: { search: "network", create_page: "external_effect" },
};
const SIGNED_LINEAR = {
	name: "linear",
	source: "custom",
	transport: "http",
	url: "https://mcp.linear.app/mcp",
	oauth: {},
	tools: { list_issues: "network" },
};
/** Theo with `notion` signed in to and `linear`'s sign-in ended by the service. */
const SIGNED_IN_TEAM = theoWith([SIGNED_NOTION, SIGNED_LINEAR]);
const SIGNED_IN_ROWS = [
	{
		agent: "theo",
		server: "notion",
		state: "connected",
		auth: "oauth",
		revokes: true,
		stored_in: "keychain",
	},
	{
		agent: "theo",
		server: "linear",
		state: "sign_in_again",
		auth: "oauth",
		revokes: false,
		stored_in: "keychain",
	},
];

/** Theo's page with the signed-in team, answered. */
async function openedSignedIn(rows: object[] = SIGNED_IN_ROWS) {
	const { container, socket } = await renderApp("/team/theo");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", teamGot(SIGNED_IN_TEAM, rows));
	await answerQuery(s, "models.list", { models: [] });
	await screen.findByRole("heading", { name: "Theo, your Developer" });
	return { container, s };
}

/** ConnectorAdd at step 1 for a web address, `url` typed and Next pressed, no key typed. */
async function askedToSignIn(url = "https://mcp.notion.com/mcp") {
	const page = await opened([], true);
	fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
	fireEvent.click(screen.getByRole("button", { name: en.connectorCustomAdd }));
	const dialog = await screen.findByRole("dialog", {
		name: "Add a custom connector to Theo",
	});
	fireEvent.change(within(dialog).getByLabelText(en.addName), {
		target: { value: "notion" },
	});
	fireEvent.click(
		within(dialog).getByRole("radio", { name: new RegExp(en.addUrlChoice) }),
	);
	fireEvent.change(within(dialog).getByLabelText(en.addUrl), {
		target: { value: url },
	});
	fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
	const asked = await sent(page.s, "connector.sign_in");
	return { ...page, dialog, asked };
}

/** The service offers signing in at `issuer`. */
const offered = (
	s: FakeSocket,
	asked: Parameters<FakeSocket["reply"]>[0],
	issuer: string,
) =>
	s.reply(asked, {
		attempt: ATTEMPT,
		authorize_url: `${issuer}/authorize?state=abc`,
		issuer,
	});

describe("signing in to a service", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.restoreAllMocks();
		localStorage.clear();
	});

	it("connector_add_offers_sign_in_when_the_service_has_one", async () => {
		const { container, s, dialog, asked } = await askedToSignIn();
		// The probe names the server and its sign-in settings, with no key and no Authorization header.
		expect(asked.params).toEqual({
			agent: "theo",
			server: {
				name: "notion",
				transport: "http",
				url: "https://mcp.notion.com/mcp",
				oauth: {},
			},
		});
		await offered(s, asked, "https://mcp.notion.com");
		expect(
			await within(dialog).findByRole("button", {
				name: "Sign in with mcp.notion.com",
			}),
		).toBeTruthy();
		expect(
			within(dialog).getByText("mcp.notion.com lets you sign in."),
		).toBeTruthy();
		expect(within(dialog).getByText(en.addSignInNote)).toBeTruthy();
		expect(
			within(dialog).getByRole("button", { name: en.addUseAKey }),
		).toBeTruthy();
		// No key field shows, and nothing was asked of the server's tools yet.
		expect(within(dialog).queryByLabelText(en.addKeyName)).toBeNull();
		expect(within(dialog).queryByLabelText(en.addHeader)).toBeNull();
		expect(s.calls("connector.tools")).toHaveLength(0);
		await expectNoAxeViolations(container);
	});

	it("connector_add_names_who_signs_you_in", async () => {
		const { container, s, dialog, asked } = await askedToSignIn(
			"https://mcp.stripe.com",
		);
		await offered(s, asked, "https://access.stripe.com");
		expect(
			await within(dialog).findByRole("button", {
				name: "Sign in with access.stripe.com",
			}),
		).toBeTruthy();
		expect(within(dialog).getByText("for mcp.stripe.com")).toBeTruthy();
		expect(
			within(dialog).getByText("mcp.stripe.com lets you sign in."),
		).toBeTruthy();
		await expectNoAxeViolations(container);
		cleanup();
		localStorage.clear();
		// Where the two hosts are the same, no "for" line says it twice.
		const same = await askedToSignIn();
		await offered(same.s, same.asked, "https://mcp.notion.com");
		await within(same.dialog).findByRole("button", {
			name: "Sign in with mcp.notion.com",
		});
		expect(within(same.dialog).queryByText(/^for /)).toBeNull();
	});

	it("connector_add_lets_a_key_be_used_where_sign_in_is_offered", async () => {
		const { s, dialog, asked } = await askedToSignIn();
		await offered(s, asked, "https://mcp.notion.com");
		fireEvent.click(
			await within(dialog).findByRole("button", { name: en.addUseAKey }),
		);
		fireEvent.change(await within(dialog).findByLabelText(en.addKeyName), {
			target: { value: "NOTION_KEY" },
		});
		fireEvent.change(within(dialog).getByLabelText(en.addKeyValue), {
			target: { value: "secret-1" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		const listing = await sent(s, "connector.tools");
		const params = listing.params as {
			keys: Record<string, string>;
			attempt?: string;
			server: { oauth?: unknown };
		};
		expect(params.keys).toEqual({ NOTION_KEY: "secret-1" });
		expect(params.attempt).toBeUndefined();
		expect(params.server.oauth).toBeUndefined();
		expect(s.calls("connector.sign_in")).toHaveLength(1);
	});

	it("connector_add_ends_the_sign_in_when_cancelled", async () => {
		vi.spyOn(window, "open").mockReturnValue(null);
		const { s, dialog, asked } = await askedToSignIn();
		await offered(s, asked, "https://mcp.notion.com");
		fireEvent.click(
			await within(dialog).findByRole("button", {
				name: "Sign in with mcp.notion.com",
			}),
		);
		expect(
			await within(dialog).findByText(
				"Waiting for you to sign in to mcp.notion.com…",
			),
		).toBeTruthy();
		expect(s.calls("connector.sign_in_cancel")).toHaveLength(0);
		fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
		// The daemon is told, so a yes given on the service's page after this is never kept.
		const cancelled = await sent(s, "connector.sign_in_cancel");
		expect(cancelled.params).toEqual({ attempt: ATTEMPT });
		expect(s.calls("connector.sign_in_cancel")).toHaveLength(1);
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("connector_add_ends_the_sign_in_when_the_dialog_is_closed", async () => {
		const { s, dialog, asked } = await askedToSignIn();
		await offered(s, asked, "https://mcp.notion.com");
		await within(dialog).findByRole("button", {
			name: "Sign in with mcp.notion.com",
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		const cancelled = await sent(s, "connector.sign_in_cancel");
		expect(cancelled.params).toEqual({ attempt: ATTEMPT });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("connector_add_ends_the_sign_in_when_a_key_is_used_instead", async () => {
		const { s, dialog, asked } = await askedToSignIn();
		await offered(s, asked, "https://mcp.notion.com");
		fireEvent.click(
			await within(dialog).findByRole("button", { name: en.addUseAKey }),
		);
		const cancelled = await sent(s, "connector.sign_in_cancel");
		expect(cancelled.params).toEqual({ attempt: ATTEMPT });
	});

	it("connector_add_ends_the_sign_in_when_the_address_changes", async () => {
		const { s, dialog, asked } = await askedToSignIn();
		await offered(s, asked, "https://mcp.notion.com");
		await within(dialog).findByRole("button", {
			name: "Sign in with mcp.notion.com",
		});
		// The sign-in was for the old address: it ends, and the user signs in again.
		fireEvent.change(within(dialog).getByLabelText(en.addUrl), {
			target: { value: "https://mcp.notion.com/other" },
		});
		const cancelled = await sent(s, "connector.sign_in_cancel");
		expect(cancelled.params).toEqual({ attempt: ATTEMPT });
		expect(s.calls("connector.sign_in_cancel")).toHaveLength(1);
	});

	it("connector_add_has_no_sign_in_to_end_when_none_was_started", async () => {
		const { s, dialog } = await startedAdding();
		fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("connector.sign_in_cancel")).toHaveLength(0);
	});

	it("connector_add_from_a_kit_ends_the_sign_in_when_cancelled", async () => {
		vi.spyOn(window, "open").mockReturnValue(null);
		const { s } = await openedWithKit([KIT_LINEAR], [], []);
		fireEvent.click(
			within(kitRow("Linear")).getByRole("button", { name: "Connect Linear" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Linear to Theo",
		});
		await offered(
			s,
			await sent(s, "connector.sign_in"),
			"https://linear.example",
		);
		fireEvent.click(
			await within(dialog).findByRole("button", {
				name: "Sign in with Linear",
			}),
		);
		expect(
			await within(dialog).findByText("Waiting for you to sign in to Linear…"),
		).toBeTruthy();
		fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
		const cancelled = await sent(s, "connector.sign_in_cancel");
		expect(cancelled.params).toEqual({ attempt: ATTEMPT });
		expect(s.calls("connector.sign_in_cancel")).toHaveLength(1);
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("connector_add_from_a_kit_has_no_sign_in_to_end_when_it_takes_a_key", async () => {
		const { s } = await openedWithKit([KIT_NOTION], [], []);
		fireEvent.click(
			within(kitRow("Notion")).getByRole("button", { name: "Connect Notion" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Notion to Theo",
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("connector.sign_in_cancel")).toHaveLength(0);
	});

	it("connector_add_waits_for_the_sign_in_then_labels", async () => {
		const open = vi.spyOn(window, "open").mockReturnValue(null);
		const { container, s, dialog, asked } = await askedToSignIn();
		await offered(s, asked, "https://mcp.notion.com");
		const button = await within(dialog).findByRole("button", {
			name: "Sign in with mcp.notion.com",
		});
		vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
		fireEvent.click(button);
		// Opened in the click itself, with no call in between, so a pop-up blocker lets it through.
		expect(open).toHaveBeenCalledWith(
			"https://mcp.notion.com/authorize?state=abc",
			"_blank",
			"noopener",
		);
		expect(s.calls("connector.sign_in_status")).toHaveLength(0);
		expect(
			within(dialog).getByText("Waiting for you to sign in to mcp.notion.com…"),
		).toBeTruthy();
		expect(
			within(dialog).getByRole("button", { name: en.addOpenAgain }),
		).toBeTruthy();

		// Asked every 2 seconds, until the service has said yes.
		await act(() => vi.advanceTimersByTimeAsync(1900));
		expect(s.calls("connector.sign_in_status")).toHaveLength(0);
		await act(() => vi.advanceTimersByTimeAsync(200));
		// (Read directly: the library's waiting needs the timers this test has faked.)
		const first = s.calls("connector.sign_in_status")[0] as NonNullable<
			ReturnType<FakeSocket["calls"]>[number]
		>;
		expect(first.params).toEqual({ attempt: ATTEMPT });
		await s.reply(first, { state: "waiting" });
		await act(() => vi.advanceTimersByTimeAsync(2000));
		const second = s.calls("connector.sign_in_status")[1] as NonNullable<
			ReturnType<FakeSocket["calls"]>[number]
		>;
		expect(second).toBeDefined();
		await s.reply(second, { state: "signed_in" });
		vi.useRealTimers();
		expect(
			await within(dialog).findByText("Signed in to mcp.notion.com."),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(within(dialog).getByRole("button", { name: en.addNext }));
		const listing = await sent(s, "connector.tools");
		expect(listing.params).toEqual({
			agent: "theo",
			server: {
				name: "notion",
				transport: "http",
				url: "https://mcp.notion.com/mcp",
				oauth: {},
			},
			attempt: ATTEMPT,
		});
		await s.reply(listing, {
			tools: [
				{ name: "search", description: "Search.", usable: true },
				{ name: "create_page", description: "Make a page.", usable: true },
			],
		});
		await within(dialog).findByRole("group", { name: "search" });
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Add notion to Theo" }),
		);
		const connect = await sent(s, "connector.connect");
		const params = connect.params as Record<string, unknown>;
		expect(params.attempt).toBe(ATTEMPT);
		expect(params.keys).toBeUndefined();
		await s.reply(connect, {
			stored_in: "keychain",
			tools: { search: "external_effect", create_page: "external_effect" },
		});
		expect(
			await within(dialog).findByText("notion is added to Theo"),
		).toBeTruthy();
		expect(within(dialog).getByText(en.addSignedIn)).toBeTruthy();
		expect(
			within(dialog).getByText(
				"Theo uses Notion as you. Farik keeps the sign-in in your keychain.",
			),
		).toBeTruthy();
		expect(
			within(dialog).getByText(
				"Only Theo has notion. To give it to someone else, add it from their page, and sign in again there.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
		// The sign-in was used up by connecting: closing now has nothing left to end.
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("connector.sign_in_cancel")).toHaveLength(0);
	});

	it("connector_add_waiting_is_accessible", async () => {
		vi.spyOn(window, "open").mockReturnValue(null);
		const { container, s, dialog, asked } = await askedToSignIn();
		await offered(s, asked, "https://mcp.notion.com");
		fireEvent.click(
			await within(dialog).findByRole("button", {
				name: "Sign in with mcp.notion.com",
			}),
		);
		expect(
			within(dialog).getByText("Waiting for you to sign in to mcp.notion.com…"),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it.each([
		[
			"access_denied",
			"You said no on mcp.notion.com’s page, so Farik isn’t connected.",
		],
		["sign_in_timed_out", "The sign-in took longer than 10 minutes."],
		[
			"sign_in_mismatch",
			"Something didn’t match on the way back from mcp.notion.com, so Farik stopped to keep you safe.",
		],
		[
			"sign_in_failed",
			"mcp.notion.com didn’t finish the sign-in. Try again, or use a key if it gives you one.",
		],
	])("connector_add_says_why_a_sign_in_failed_%s", async (code, sentence) => {
		vi.spyOn(window, "open").mockReturnValue(null);
		const { container, s, dialog, asked } = await askedToSignIn();
		await offered(s, asked, "https://mcp.notion.com");
		const button = await within(dialog).findByRole("button", {
			name: "Sign in with mcp.notion.com",
		});
		vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
		fireEvent.click(button);
		await act(() => vi.advanceTimersByTimeAsync(2100));
		const poll = s.calls("connector.sign_in_status")[0] as NonNullable<
			ReturnType<FakeSocket["calls"]>[number]
		>;
		expect(poll).toBeDefined();
		await s.reply(poll, {
			state: "failed",
			reason: { code, message: "whatever the daemon says" },
		});
		vi.useRealTimers();
		const line = await within(dialog).findByText(sentence);
		expect(line.getAttribute("role")).toBe("alert");
		// The daemon's own words are never shown, and "Try again" asks the service again.
		expect(within(dialog).queryByText(/whatever the daemon says/)).toBeNull();
		await expectNoAxeViolations(container);
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.addTryAgain }),
		);
		await sent(s, "connector.sign_in", 2);
	});

	it("connector_add_falls_back_to_a_key", async () => {
		// The service offers no sign-in: the key fields, unchanged, and nothing said.
		const one = await askedToSignIn();
		await one.s.fail(
			one.asked,
			-32005,
			"sign_in_not_offered: this server does not offer signing in",
		);
		expect(
			await within(one.dialog).findByLabelText(en.addKeyName),
		).toBeTruthy();
		expect(within(one.dialog).queryByText(/sign in/i)).toBeNull();
		expect(within(one.dialog).queryByRole("alert")).toBeNull();
		await expectNoAxeViolations(one.container);
		cleanup();
		localStorage.clear();

		for (const [code, sentence] of [
			[
				"sign_in_not_supported",
				"api.githubcopilot.com doesn’t let Farik sign in by itself yet. If it gives you a key, paste it below.",
			],
			[
				"sign_in_failed",
				"Farik couldn’t sign in to api.githubcopilot.com. If it gives you a key, paste it below.",
			],
		] as const) {
			const page = await askedToSignIn("https://api.githubcopilot.com/mcp");
			await page.s.fail(page.asked, -32005, `${code}: the daemon's words`);
			expect(await within(page.dialog).findByText(sentence)).toBeTruthy();
			expect(within(page.dialog).getByLabelText(en.addKeyName)).toBeTruthy();
			expect(within(page.dialog).queryByText(/the daemon's words/)).toBeNull();
			await expectNoAxeViolations(page.container);
			// And Next now lists with the keys, never asking the service to sign in again.
			fireEvent.click(
				within(page.dialog).getByRole("button", { name: en.addNext }),
			);
			await sent(page.s, "connector.tools");
			expect(page.s.calls("connector.sign_in")).toHaveLength(1);
			cleanup();
			localStorage.clear();
		}
	});

	it("connector_add_lists_at_once_when_nothing_is_offered_or_needed", async () => {
		// A web address that offers no sign-in, with no key typed and none asked for: the tools
		// are listed without a second Next.
		const one = await askedToSignIn();
		await one.s.fail(
			one.asked,
			-32005,
			"sign_in_not_offered: this server does not offer signing in",
		);
		const listing = await sent(one.s, "connector.tools");
		expect(listing.params.keys).toEqual({});
		expect(one.s.calls("connector.sign_in")).toHaveLength(1);
	});

	it("agent_edit_shows_signed_in_and_sign_in_again", async () => {
		const { container, s } = await openedSignedIn();
		const notion = row("notion");
		expect(within(notion).getByText(en.connectorHttp)).toBeTruthy();
		expect(
			within(notion).getByText("Signed in to mcp.notion.com"),
		).toBeTruthy();
		expect(
			within(notion).getByText(
				"2 tools: 1 Only reads, 1 Changes things, asks you",
			),
		).toBeTruthy();
		expect(within(notion).queryByText(/keys are in/)).toBeNull();
		expect(
			within(notion).queryByRole("button", { name: en.connectorSignInAgain }),
		).toBeNull();
		const linear = row("linear");
		expect(
			within(linear).getByText(
				"mcp.linear.app ended Farik’s sign-in. Sign in again to use it.",
			),
		).toBeTruthy();
		expect(within(linear).queryByText(en.connectorAgain)).toBeNull();
		await expectNoAxeViolations(container);

		fireEvent.click(
			within(linear).getByRole("button", { name: en.connectorSignInAgain }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Add linear to Theo",
		});
		// Opened at the sign-in, filled in from the team file: the service is asked at once.
		const asked = await sent(s, "connector.sign_in");
		expect(asked.params).toEqual({
			agent: "theo",
			server: {
				name: "linear",
				transport: "http",
				url: "https://mcp.linear.app/mcp",
				oauth: {},
			},
		});
		await offered(s, asked, "https://mcp.linear.app");
		expect(
			await within(dialog).findByRole("button", {
				name: "Sign in with mcp.linear.app",
			}),
		).toBeTruthy();
		expect(
			within(dialog).getByText(
				"mcp.linear.app ended Farik’s sign-in. Sign in again to use linear.",
			),
		).toBeTruthy();
		expect(
			(within(dialog).getByLabelText(en.addName) as HTMLInputElement).value,
		).toBe("linear");
		await expectNoAxeViolations(container);
	});

	it("agent_edit_remove_says_the_service_is_asked_to_forget", async () => {
		const { container } = await openedSignedIn();
		for (const [server, words] of [
			[
				"notion",
				"Farik deletes the sign-in from your keychain and asks mcp.notion.com to forget it.",
			],
			[
				"linear",
				"Farik deletes the sign-in from your keychain. To remove Farik completely, also remove it in mcp.linear.app’s settings.",
			],
		] as const) {
			fireEvent.click(
				within(row(server)).getByRole("button", { name: `Remove ${server}` }),
			);
			const dialog = await screen.findByRole("dialog", {
				name: `Remove ${server} from Theo?`,
			});
			expect(within(dialog).getByText(words)).toBeTruthy();
			expect(within(dialog).queryByText(/type the keys/)).toBeNull();
			await expectNoAxeViolations(container);
			fireEvent.click(
				within(dialog).getByRole("button", { name: en.connectorKeep }),
			);
			await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		}
	});
});

/** What a role's kit offers Theo, as `team.get` answers it (ADR 0036). */
const KIT_NOTION = {
	name: "notion",
	title: "Notion",
	about: "Notion is where your team keeps docs, notes and plans.",
	why: "Reads your product docs, so plans start from what you already wrote.",
	setup:
		"On Notion’s page, choose “New integration”, name it Farik and pick your workspace. Copy the value labelled “Internal Integration Secret”.",
	key_page: "https://www.notion.so/profile/integrations",
	labels: {
		search: "search pages",
		read_page: "read a page",
		create_page: "create a page",
		delete_page: "delete a page",
	},
	auth: "keys",
	credential_keys: ["NOTION_KEY"],
};
const KIT_LINEAR = {
	name: "linear",
	title: "Linear",
	about: "Linear is where your team keeps its backlog.",
	why: "Reads your backlog, so a request can start from an issue you already filed.",
	setup: "Sign in with your Linear account.",
	labels: {},
	auth: "oauth",
	credential_keys: [],
};
const KIT_POSTHOG = {
	name: "posthog",
	title: "PostHog",
	about: "PostHog shows how people use your product.",
	why: "Reads how people use your product, so priorities follow what they do.",
	setup: "Make a key on PostHog’s page and paste it.",
	key_page: "https://posthog.example/keys",
	labels: {},
	auth: "keys",
	credential_keys: ["POSTHOG_KEY"],
};
/** A kit service as `team.yaml` holds it once connected: the kit's tags are its tools. */
const kitEntry = (name: string, transport: "http" | "stdio" = "stdio") => ({
	name,
	source: "kit",
	transport,
	...(transport === "http"
		? { url: `https://${name}.example/mcp`, oauth: {} }
		: {
				command: `${name}-server`,
				credential_keys: [`${name.toUpperCase()}_KEY`],
			}),
	tools: { search: "network", create_page: "external_effect" },
});
const KIT_STATES = [
	{
		agent: "theo",
		server: "linear",
		state: "connected",
		auth: "oauth",
		source: "kit",
		revokes: true,
		stored_in: "keychain",
	},
	{
		agent: "theo",
		server: "posthog",
		state: "connect_again",
		auth: "keys",
		source: "kit",
	},
	{
		agent: "theo",
		server: "plausible",
		state: "not_in_kit",
		auth: "keys",
		source: "kit",
	},
];
const kitTeam = (servers: object[]) => theoWith(servers);

/** Theo's page with a kit offering `services`, Theo holding `held`, whose states are `states`. */
async function openedWithKit(
	services: object[],
	held: object[],
	states: object[],
) {
	const { container, socket } = await renderApp("/team/theo");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", {
		...teamGot(kitTeam(held), states),
		kits: [{ role: "software_developer", connectors: services }],
	});
	await answerQuery(s, "models.list", { models: [] });
	await screen.findByRole("heading", { name: "Theo, your Developer" });
	return { container, s };
}

/** The row of the kit service `title` in "From the Developer’s kit". */
const kitRow = (title: string) => {
	const section = screen.getByRole("list", {
		name: "From the Developer’s kit",
	});
	return within(section)
		.getByText(title, { selector: "strong" })
		.closest("li") as HTMLElement;
};

describe("a role's kit on the agent page", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.restoreAllMocks();
		localStorage.clear();
	});

	it("agent_edit_lists_the_kit_above_your_own", async () => {
		const { container } = await openedWithKit(
			[KIT_NOTION, KIT_LINEAR, KIT_POSTHOG],
			[
				AIRTABLE,
				kitEntry("linear", "http"),
				kitEntry("posthog"),
				kitEntry("plausible"),
			],
			[...CONNECTORS.slice(0, 1), ...KIT_STATES],
		);
		const kit = screen.getByRole("list", { name: "From the Developer’s kit" });
		const yours = screen.getByRole("list", { name: en.connectorsYours });
		// The kit comes first.
		expect(
			kit.compareDocumentPosition(yours) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		// Not connected: the service, why, and Connect.
		const notion = kitRow("Notion");
		expect(within(notion).getByText(KIT_NOTION.why)).toBeTruthy();
		expect(
			within(notion).getByRole("button", { name: "Connect Notion" }),
		).toBeTruthy();
		expect(within(notion).queryByRole("button", { name: /Remove/ })).toBeNull();
		// Connected: signed in, and Remove.
		const linear = kitRow("Linear");
		expect(within(linear).getByText(en.kitConnected)).toBeTruthy();
		expect(within(linear).getByText("Signed in to Linear.")).toBeTruthy();
		expect(
			within(linear).getByRole("button", { name: "Remove linear" }),
		).toBeTruthy();
		expect(
			within(linear).queryByRole("button", { name: /Connect/ }),
		).toBeNull();
		// Farik updated it: Connect again, with the reason, and Remove.
		const posthog = kitRow("PostHog");
		expect(within(posthog).getByText(en.kitAgain)).toBeTruthy();
		expect(
			within(posthog).getByRole("button", { name: "Connect again PostHog" }),
		).toBeTruthy();
		expect(
			within(posthog).getByRole("button", { name: "Remove posthog" }),
		).toBeTruthy();
		// Farik no longer offers it: Remove alone, whatever the entry says.
		const gone = kitRow("plausible");
		expect(within(gone).getByText(en.kitGone)).toBeTruthy();
		expect(
			within(gone).getByRole("button", { name: "Remove plausible" }),
		).toBeTruthy();
		expect(within(gone).queryByRole("button", { name: /Connect/ })).toBeNull();
		// A kit service is not also "Added by you".
		expect(
			within(yours).queryByText("linear", { selector: "strong" }),
		).toBeNull();
		expect(
			within(yours).getByText("airtable", { selector: "strong" }),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("connector_add_from_a_kit_skips_labelling", async () => {
		const { container, s } = await openedWithKit([KIT_NOTION], [], []);
		fireEvent.click(
			within(kitRow("Notion")).getByRole("button", { name: "Connect Notion" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Notion to Theo",
		});
		expect(within(dialog).getByText(KIT_NOTION.about)).toBeTruthy();
		expect(within(dialog).getByText(KIT_NOTION.why)).toBeTruthy();
		// The service's own label, quoted in its steps, is shown as it is.
		expect(within(dialog).getByText(KIT_NOTION.setup)).toBeTruthy();
		const link = within(dialog).getByRole("link", { name: /Get your key/ });
		expect(link.getAttribute("href")).toBe(KIT_NOTION.key_page);
		expect(link.getAttribute("target")).toBe("_blank");
		expect(link.getAttribute("rel")).toContain("noopener");
		const key = within(dialog).getByLabelText(
			"Your Notion key",
		) as HTMLInputElement;
		expect(key.type).toBe("password");
		await expectNoAxeViolations(container);
		fireEvent.change(key, { target: { value: "ntn-secret-1" } });
		fireEvent.click(within(dialog).getByRole("button", { name: "Connect" }));
		const connect = await sent(s, "connector.connect");
		expect(connect.params).toEqual({
			agent: "theo",
			server: { name: "notion", source: "kit" },
			keys: { NOTION_KEY: "ntn-secret-1" },
			tags: {},
		});
		// Nothing was listed for the user to label, and no label is asked for.
		expect(s.calls("connector.tools")).toHaveLength(0);
		expect(within(dialog).queryByRole("radio")).toBeNull();
		expect(container.innerHTML).not.toContain("ntn-secret-1");
		await s.reply(connect, {
			stored_in: "keychain",
			tools: { search: "network" },
		});
		expect(
			await within(dialog).findByText("Notion is connected to Theo"),
		).toBeTruthy();
	});

	it("connector_add_from_a_kit_signs_in_when_the_kit_does", async () => {
		const open = vi.spyOn(window, "open").mockReturnValue(null);
		const { container, s } = await openedWithKit([KIT_LINEAR], [], []);
		fireEvent.click(
			within(kitRow("Linear")).getByRole("button", { name: "Connect Linear" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Linear to Theo",
		});
		// No key field: the button names the service, and the page is asked for at once.
		expect(within(dialog).queryByLabelText(/key/i)).toBeNull();
		const asked = await sent(s, "connector.sign_in");
		expect(asked.params).toEqual({
			agent: "theo",
			server: { name: "linear", source: "kit" },
		});
		await s.reply(asked, {
			attempt: ATTEMPT,
			authorize_url: "https://linear.example/authorize?state=abc",
			issuer: "https://linear.example",
		});
		const button = await within(dialog).findByRole("button", {
			name: "Sign in with Linear",
		});
		await expectNoAxeViolations(container);
		vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
		fireEvent.click(button);
		expect(open).toHaveBeenCalledWith(
			"https://linear.example/authorize?state=abc",
			"_blank",
			"noopener",
		);
		await act(() => vi.advanceTimersByTimeAsync(2100));
		const status = s.calls("connector.sign_in_status")[0] as NonNullable<
			ReturnType<FakeSocket["calls"]>[number]
		>;
		await s.reply(status, { state: "signed_in" });
		vi.useRealTimers();
		// Signed in is connected: nothing to label.
		const connect = await sent(s, "connector.connect");
		expect(connect.params).toEqual({
			agent: "theo",
			server: { name: "linear", source: "kit" },
			attempt: ATTEMPT,
			tags: {},
		});
		await s.reply(connect, {
			stored_in: "keychain",
			tools: { search: "network" },
		});
		expect(
			await within(dialog).findByText("Linear is connected to Theo"),
		).toBeTruthy();
		expect(s.calls("connector.tools")).toHaveLength(0);
	});

	it("connector_add_from_a_kit_says_why_a_signed_in_connect_failed", async () => {
		const open = vi.spyOn(window, "open").mockReturnValue(null);
		const { s } = await openedWithKit([KIT_LINEAR], [], []);
		fireEvent.click(
			within(kitRow("Linear")).getByRole("button", { name: "Connect Linear" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Linear to Theo",
		});
		const asked = await sent(s, "connector.sign_in");
		await s.reply(asked, {
			attempt: ATTEMPT,
			authorize_url: "https://linear.example/authorize?state=abc",
			issuer: "https://linear.example",
		});
		const button = await within(dialog).findByRole("button", {
			name: "Sign in with Linear",
		});
		vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
		fireEvent.click(button);
		expect(open).toHaveBeenCalled();
		await act(() => vi.advanceTimersByTimeAsync(2100));
		const status = s.calls("connector.sign_in_status")[0] as NonNullable<
			ReturnType<FakeSocket["calls"]>[number]
		>;
		await s.reply(status, { state: "signed_in" });
		vi.useRealTimers();
		const connect = await sent(s, "connector.connect");
		await s.fail(
			connect,
			-32005,
			"connector_not_in_kit: linear is not in the kit",
		);
		expect(
			await within(dialog).findByText(
				t("kitChanged", { service: "Linear", name: "Theo" }),
			),
		).toBeTruthy();
		expect(within(dialog).queryByText(en.addSignInTimedOut)).toBeNull();
	});

	it("connector_add_from_a_kit_says_the_service_did_not_answer", async () => {
		const { s } = await openedWithKit([KIT_NOTION], [], []);
		fireEvent.click(
			within(kitRow("Notion")).getByRole("button", { name: "Connect Notion" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Notion to Theo",
		});
		fireEvent.change(within(dialog).getByLabelText("Your Notion key"), {
			target: { value: "k" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Connect" }));
		await s.fail(
			await sent(s, "connector.connect"),
			-32005,
			"the server did not answer within thirty seconds",
		);
		expect(
			await within(dialog).findByText(
				"Notion did not answer within thirty seconds. Try again in a minute.",
			),
		).toBeTruthy();
		expect(
			within(dialog).queryByText(en.addTimeout.replace("{server}", "Notion")),
		).toBeNull();
	});

	it("done_groups_the_tools_by_what_the_agent_may_do", async () => {
		const { container, s } = await openedWithKit([KIT_NOTION], [], []);
		fireEvent.click(
			within(kitRow("Notion")).getByRole("button", { name: "Connect Notion" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Notion to Theo",
		});
		fireEvent.change(within(dialog).getByLabelText("Your Notion key"), {
			target: { value: "k" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Connect" }));
		await s.reply(await sent(s, "connector.connect"), {
			stored_in: "keychain",
			tools: {
				search: "network",
				"API-post-search": "network",
				create_page: "external_effect",
				delete_page: "denied",
			},
		});
		await within(dialog).findByText("Notion is connected to Theo");
		const list = (heading: string) => {
			const group = within(dialog)
				.getByText(heading)
				.closest("div") as HTMLElement;
			return within(group)
				.getAllByRole("listitem")
				.map((item) => item.textContent);
		};
		// Each tool by its kit label, or by its name with `_` and `-` read as spaces.
		expect(list("Theo can now")).toEqual(["search pages", "API post search"]);
		expect(list("Theo asks you first before")).toEqual(["create a page"]);
		expect(list("Farik never offers")).toEqual(["delete a page"]);
		expect(
			within(dialog).getByText(
				"Theo’s key is kept in your computer’s keychain. Theo never sees it: Farik hands it to Notion.",
			),
		).toBeTruthy();
		expect(
			within(dialog).getByText(
				"Only Theo has Notion. To give it to someone else, connect it from their page.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("saving_the_agent_keeps_a_kit_connector_connected_meanwhile", async () => {
		const { s } = await openedWithKit([KIT_NOTION], [AIRTABLE], []);
		editPersona();
		fireEvent.click(
			within(kitRow("Notion")).getByRole("button", { name: "Connect Notion" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect Notion to Theo",
		});
		fireEvent.change(within(dialog).getByLabelText("Your Notion key"), {
			target: { value: "k" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Connect" }));
		await s.reply(await sent(s, "connector.connect"), {
			stored_in: "keychain",
			tools: { search: "network" },
		});
		const before = teamAskedTimes(s);
		fireEvent.click(
			await within(dialog).findByRole("button", { name: "Back to Theo" }),
		);
		await waitFor(() => expect(teamAskedTimes(s)).toBeGreaterThan(before));
		await answerQuery(s, "team.get", {
			...teamGot(kitTeam([AIRTABLE, kitEntry("notion")]), [
				{
					agent: "theo",
					server: "notion",
					state: "connected",
					auth: "keys",
					source: "kit",
					stored_in: "keychain",
				},
			]),
			kits: [{ role: "software_developer", connectors: [KIT_NOTION] }],
		});
		await waitFor(() =>
			expect(within(kitRow("Notion")).getByText(en.kitConnected)).toBeTruthy(),
		);
		const theo = await savedTheo(s);
		expect(theo?.persona).toBe("Short and kind.");
		expect(theo?.mcp_servers?.map((c) => c.name)).toEqual([
			"airtable",
			"notion",
		]);
	});

	it("kit_screens_never_name_the_plumbing", () => {
		const plumbing = /\b(mcp|oauth|token)/i;
		// Farik's own words for these screens.
		const own = Object.entries(en)
			.filter(([key]) => key.startsWith("kit"))
			.map(([key, words]) => [key, words]);
		expect(own.length).toBeGreaterThan(10);
		for (const [key, words] of own) expect(words, key).not.toMatch(plumbing);
		// The kit's copy, outside a service's label quoted in its steps.
		const unquoted = (words: string) =>
			words.replace(/[‘“][^’”\n]{0,60}[’”]/g, " ");
		for (const service of [KIT_NOTION, KIT_LINEAR, KIT_POSTHOG]) {
			for (const field of ["title", "about", "why"] as const)
				expect(service[field]).not.toMatch(plumbing);
			expect(unquoted(service.setup)).not.toMatch(plumbing);
		}
	});
});

// ---- Farik's own apps: GitHub, signed in to with a code (phase 7 step 03b) ----

const GITHUB_ADDRESS = "https://api.githubcopilot.com/mcp/";
const INSTALL_URL = "https://github.com/apps/farik/installations/new";
const GITHUB_SETTINGS = "https://github.com/settings/apps/authorizations";
/** What `connector.sign_in` answers for Farik's GitHub App: the page to type the code on, and the code. */
const GITHUB_OFFER = {
	attempt: ATTEMPT,
	authorize_url: "https://github.com/login/device",
	issuer: "https://github.com/login/oauth",
	provider: "GitHub",
	user_code: "WDJB-MJHT",
	install_url: INSTALL_URL,
};
const SIGNED_GITHUB = {
	name: "github",
	source: "custom",
	transport: "http",
	url: GITHUB_ADDRESS,
	oauth: {},
	tools: { search_code: "network", get_file_contents: "network" },
};
const GITHUB_ROW = {
	agent: "theo",
	server: "github",
	state: "connected",
	auth: "oauth",
	revokes: false,
	stored_in: "keychain",
	provider: "GitHub",
	settings_url: GITHUB_SETTINGS,
};
/** A second GitHub server of Theo's, whose sign-in GitHub ended. */
const ENDED_GITHUB = { ...SIGNED_GITHUB, name: "github_work" };
const ENDED_GITHUB_ROW = {
	...GITHUB_ROW,
	server: "github_work",
	state: "sign_in_again",
};

describe("signing in with one of Farik's own apps", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.restoreAllMocks();
		Reflect.deleteProperty(navigator, "clipboard");
		localStorage.clear();
	});

	it("connector_add_signs_in_with_github_by_a_code", async () => {
		const open = vi.spyOn(window, "open").mockReturnValue(null);
		const writeText = vi.fn().mockResolvedValue(undefined);
		Object.defineProperty(navigator, "clipboard", {
			value: { writeText },
			configurable: true,
		});
		const { container, s, dialog, asked } = await askedToSignIn(GITHUB_ADDRESS);
		await s.reply(asked, GITHUB_OFFER);
		const button = await within(dialog).findByRole("button", {
			name: "Sign in with GitHub",
		});
		vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
		fireEvent.click(button);
		// The code shows, with the warning under it; nothing is opened until the user presses Open.
		expect(open).not.toHaveBeenCalled();
		expect(within(dialog).getByText("Enter this code on GitHub:")).toBeTruthy();
		expect(within(dialog).getByText("WDJB-MJHT")).toBeTruthy();
		expect(
			within(dialog).getByText(
				"Only enter a code that this page shows you. Farik never sends you a code in a chat.",
			),
		).toBeTruthy();
		expect(within(dialog).getByText("Waiting for you on GitHub…")).toBeTruthy();

		fireEvent.click(
			within(dialog).getByRole("button", { name: "Copy the code" }),
		);
		expect(writeText).toHaveBeenCalledWith("WDJB-MJHT");
		// Opened in the click itself, with no call in between, so a pop-up blocker lets it through.
		fireEvent.click(
			within(dialog).getByRole("button", {
				name: "Open github.com/login/device",
			}),
		);
		expect(open).toHaveBeenCalledWith(
			"https://github.com/login/device",
			"_blank",
			"noopener",
		);

		// Asked every 2 seconds, until the user has said yes on GitHub.
		await act(() => vi.advanceTimersByTimeAsync(2100));
		const status = s.calls("connector.sign_in_status")[0] as NonNullable<
			ReturnType<FakeSocket["calls"]>[number]
		>;
		expect(status.params).toEqual({ attempt: ATTEMPT });
		await s.reply(status, { state: "signed_in" });
		vi.useRealTimers();
		expect(
			await within(dialog).findByText("Signed in to GitHub."),
		).toBeTruthy();
		expect(
			within(dialog).getByText(
				"To let Theo read private repositories, install Farik on them on GitHub.",
			),
		).toBeTruthy();
		const link = within(dialog).getByRole("link", {
			name: "Install Farik on GitHub",
		});
		expect(link.getAttribute("href")).toBe(INSTALL_URL);
		expect(link.getAttribute("target")).toBe("_blank");
		expect(link.getAttribute("rel")).toContain("noopener");
		// The code is no longer asked for, and Next lists the tools as after any sign-in.
		expect(within(dialog).queryByText("WDJB-MJHT")).toBeNull();
		expect(
			within(dialog).getByRole("button", { name: en.addNext }),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("connector_add_names_the_provider", async () => {
		const { container, s, dialog, asked } = await askedToSignIn(GITHUB_ADDRESS);
		await s.reply(asked, GITHUB_OFFER);
		expect(
			await within(dialog).findByRole("button", {
				name: "Sign in with GitHub",
			}),
		).toBeTruthy();
		expect(within(dialog).getByText("GitHub lets you sign in.")).toBeTruthy();
		// Farik offers this sign-in to GitHub's own address only, so no "for <host>" line says it.
		expect(within(dialog).queryByText(/^for /)).toBeNull();
		expect(
			within(dialog).getByText(
				"Farik shows you a short code to type on GitHub’s page.",
			),
		).toBeTruthy();
		expect(within(dialog).queryByText(en.addSignInNote)).toBeNull();
		expect(
			within(dialog).getByRole("button", { name: en.addUseAKey }),
		).toBeTruthy();
		await expectNoAxeViolations(container);
		// The board with the code, checked with the clock running: axe waits on timers.
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Sign in with GitHub" }),
		);
		expect(await within(dialog).findByText("WDJB-MJHT")).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("connector_add_from_a_kit_shows_the_code_when_the_service_signs_in_by_one", async () => {
		const open = vi.spyOn(window, "open").mockReturnValue(null);
		const writeText = vi.fn().mockResolvedValue(undefined);
		Object.defineProperty(navigator, "clipboard", {
			value: { writeText },
			configurable: true,
		});
		const service = { ...KIT_LINEAR, name: "github", title: "GitHub" };
		const { s } = await openedWithKit([service], [], []);
		fireEvent.click(
			within(kitRow("GitHub")).getByRole("button", { name: "Connect GitHub" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Connect GitHub to Theo",
		});
		const asked = await sent(s, "connector.sign_in");
		await s.reply(asked, GITHUB_OFFER);
		const button = await within(dialog).findByRole("button", {
			name: "Sign in with GitHub",
		});
		vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
		fireEvent.click(button);
		// The code is typed on a page the next board opens, so the button opens nothing.
		expect(open).not.toHaveBeenCalled();
		expect(within(dialog).getByText("Enter this code on GitHub:")).toBeTruthy();
		expect(within(dialog).getByText("WDJB-MJHT")).toBeTruthy();
		expect(within(dialog).getByText(en.addCodeWarning)).toBeTruthy();
		expect(
			within(dialog).getByRole("button", { name: "Copy the code" }),
		).toBeTruthy();
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Copy the code" }),
		);
		expect(writeText).toHaveBeenCalledWith("WDJB-MJHT");
		expect(open).not.toHaveBeenCalled();
		fireEvent.click(
			within(dialog).getByRole("button", {
				name: "Open github.com/login/device",
			}),
		);
		expect(open).toHaveBeenCalledTimes(1);
		expect(open).toHaveBeenCalledWith(
			"https://github.com/login/device",
			"_blank",
			"noopener",
		);
		// Saying yes on GitHub connects, as it does for any kit service that signs in.
		await act(() => vi.advanceTimersByTimeAsync(2100));
		const status = s.calls("connector.sign_in_status")[0] as NonNullable<
			ReturnType<FakeSocket["calls"]>[number]
		>;
		await s.reply(status, { state: "signed_in" });
		vi.useRealTimers();
		const connect = await sent(s, "connector.connect");
		expect(connect.params).toEqual({
			agent: "theo",
			server: { name: "github", source: "kit" },
			attempt: ATTEMPT,
			tags: {},
		});
	});

	it("connector_add_says_why_a_github_sign_in_failed", async () => {
		const { s, dialog, asked } = await askedToSignIn(GITHUB_ADDRESS);
		await s.reply(asked, GITHUB_OFFER);
		const button = await within(dialog).findByRole("button", {
			name: "Sign in with GitHub",
		});
		vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
		fireEvent.click(button);
		await act(() => vi.advanceTimersByTimeAsync(2100));
		const status = s.calls("connector.sign_in_status")[0] as NonNullable<
			ReturnType<FakeSocket["calls"]>[number]
		>;
		await s.reply(status, {
			state: "failed",
			reason: {
				code: "access_denied",
				message: "You said no on github.com’s page.",
			},
		});
		vi.useRealTimers();
		expect(
			await within(dialog).findByText(
				"You said no on GitHub’s page, so Farik isn’t connected.",
			),
		).toBeTruthy();
	});

	it("agent_edit_names_the_provider_and_github_s_settings", async () => {
		const { container, s } = await (async () => {
			const { container, socket } = await renderApp("/team/theo");
			const s = socket as FakeSocket;
			await answerStatus(s, false);
			await answerQuery(
				s,
				"team.get",
				teamGot(theoWith([SIGNED_NOTION, SIGNED_GITHUB, ENDED_GITHUB]), [
					SIGNED_IN_ROWS[0] as object,
					GITHUB_ROW,
					ENDED_GITHUB_ROW,
				]),
			);
			await answerQuery(s, "models.list", { models: [] });
			await screen.findByRole("heading", { name: "Theo, your Developer" });
			return { container, s };
		})();
		const github = row("github");
		// GitHub signed Theo in, not the address it was reached at; a service that signs in by
		// itself is still named by its address.
		expect(within(github).getByText("Signed in to GitHub")).toBeTruthy();
		expect(
			within(row("notion")).getByText("Signed in to mcp.notion.com"),
		).toBeTruthy();
		fireEvent.click(
			within(github).getByRole("button", { name: "Remove github" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Remove github from Theo?",
		});
		const settings = within(dialog).getByRole("link", {
			name: "GitHub’s settings",
		});
		expect(settings.getAttribute("href")).toBe(GITHUB_SETTINGS);
		expect(settings.getAttribute("target")).toBe("_blank");
		expect(settings.getAttribute("rel")).toContain("noopener");
		expect((settings.parentElement as HTMLElement).textContent).toBe(
			"Farik deletes the sign-in from your keychain. To remove Farik completely, also remove it in GitHub’s settings.",
		);
		expect(
			within(dialog).getByText(
				"Nobody else on the team is affected. To use it again, add it again and sign in.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		// GitHub ended the sign-in: the row and the dialog Sign in again opens say GitHub did,
		// not the address (api.githubcopilot.com) it is reached at.
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.connectorKeep }),
		);
		const ended = row("github_work");
		expect(
			within(ended).getByText(
				"GitHub ended Farik’s sign-in. Sign in again to use it.",
			),
		).toBeTruthy();
		expect(within(ended).queryByText(/api\.githubcopilot\.com/)).toBeNull();
		fireEvent.click(
			within(ended).getByRole("button", { name: en.connectorSignInAgain }),
		);
		const again = await screen.findByRole("dialog", {
			name: "Add github_work to Theo",
		});
		expect(
			within(again).getByText(
				"GitHub ended Farik’s sign-in. Sign in again to use github_work.",
			),
		).toBeTruthy();
		expect(
			within(again).queryByText(/api\.githubcopilot\.com ended/),
		).toBeNull();
		// The service is asked at once; its answer is not this test's business.
		await sent(s, "connector.sign_in");
	});

	it("agent_edit_names_a_signed_in_command_by_its_own_name_when_nothing_else_does", async () => {
		// An older daemon that does not say the provider: the connector has no web address either,
		// so Remove names it by its own name, and has no page to link to.
		const service = {
			...KIT_LINEAR,
			name: "google_ads",
			title: "Google Ads",
			auth: "oauth",
		};
		const held = {
			name: "google_ads",
			source: "kit",
			transport: "stdio",
			command: "farik",
			args: ["connector", "google-ads"],
			oauth: {},
			tools: { search: "network" },
		};
		await openedWithKit(
			[service],
			[held],
			[
				{
					agent: "theo",
					server: "google_ads",
					state: "connected",
					auth: "oauth",
					source: "kit",
					revokes: false,
					stored_in: "keychain",
				},
			],
		);
		fireEvent.click(
			within(kitRow("Google Ads")).getByRole("button", {
				name: "Remove google_ads",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Remove google_ads from Theo?",
		});
		expect(
			within(dialog).getByText(
				"Farik deletes the sign-in from your keychain. To remove Farik completely, also remove it in google_ads’s settings.",
			),
		).toBeTruthy();
		expect(within(dialog).queryByRole("link")).toBeNull();
	});

	it("agent_edit_names_the_provider_when_a_kit_s_sign_in_ended", async () => {
		// Google ended the sign-in of Google Ads: the row says Google did, not the kit's title.
		const service = {
			...KIT_LINEAR,
			name: "google_ads",
			title: "Google Ads",
			auth: "oauth",
		};
		const held = {
			name: "google_ads",
			source: "kit",
			transport: "stdio",
			command: "farik",
			args: ["connector", "google-ads"],
			oauth: {},
			tools: { search: "network" },
		};
		await openedWithKit(
			[service],
			[held],
			[
				{
					agent: "theo",
					server: "google_ads",
					state: "sign_in_again",
					auth: "oauth",
					source: "kit",
					revokes: false,
					stored_in: "keychain",
					provider: "Google",
					settings_url: "https://myaccount.google.com/connections",
				},
			],
		);
		const ads = kitRow("Google Ads");
		expect(
			within(ads).getByText(
				"Google ended Farik’s sign-in. Sign in again to use it.",
			),
		).toBeTruthy();
	});

	it("agent_edit_names_the_provider_of_a_connector_with_no_web_address", async () => {
		// Google Ads is Farik's own connector from a kit: a command, no address, signed in through
		// Google (step 08e). Its row and its Remove name Google, not the kit's title.
		const service = {
			...KIT_LINEAR,
			name: "google_ads",
			title: "Google Ads",
			auth: "oauth",
		};
		const held = {
			name: "google_ads",
			source: "kit",
			transport: "stdio",
			command: "farik",
			args: ["connector", "google-ads"],
			oauth: {},
			tools: { search: "network" },
		};
		const { container } = await openedWithKit(
			[service],
			[held],
			[
				{
					agent: "theo",
					server: "google_ads",
					state: "connected",
					auth: "oauth",
					source: "kit",
					revokes: false,
					stored_in: "keychain",
					provider: "Google",
					settings_url: "https://myaccount.google.com/connections",
				},
			],
		);
		const ads = kitRow("Google Ads");
		expect(within(ads).getByText("Signed in to Google.")).toBeTruthy();
		expect(within(ads).queryByText("Signed in to Google Ads.")).toBeNull();
		fireEvent.click(
			within(ads).getByRole("button", { name: "Remove google_ads" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Remove google_ads from Theo?",
		});
		const settings = within(dialog).getByRole("link", {
			name: "Google’s settings",
		});
		expect(settings.getAttribute("href")).toBe(
			"https://myaccount.google.com/connections",
		);
		expect((settings.parentElement as HTMLElement).textContent).toBe(
			"Farik deletes the sign-in from your keychain. To remove Farik completely, also remove it in Google’s settings.",
		);
		await expectNoAxeViolations(container);
	});
});
