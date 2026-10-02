import { expectNoAxeViolations } from "@farik/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { en } from "../strings/en.ts";
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
			target: { value: "npx -y github-connector" },
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
