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
	headers: { Authorization: "Bearer {LINEAR_KEY}" },
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

/** Theo's page, with the team and its connectors answered. */
async function opened(connectors: object[] = CONNECTORS) {
	const { container, socket } = await renderApp("/team/theo");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", {
		team: TEAM,
		agents: EFFECTIVE,
		judges: { auto: null, architect: null, scrum_master: null },
		max_agents: 7,
		connectors,
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
async function startedAdding(key = "pat-secret-1") {
	const page = await opened([]);
	fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
	fireEvent.click(screen.getByRole("button", { name: en.connectorCustomAdd }));
	const dialog = await screen.findByRole("dialog", {
		name: "Add a custom connector to Theo",
	});
	const field = (label: string) => within(dialog).getByLabelText(label);
	fireEvent.change(field(en.addName), { target: { value: "airtable" } });
	fireEvent.change(field(en.addCommand), {
		target: { value: "npx -y airtable-connector" },
	});
	fireEvent.change(field(en.addKeyName), {
		target: { value: "AIRTABLE_API_KEY" },
	});
	fireEvent.change(field(en.addKeyValue), { target: { value: key } });
	return { ...page, dialog };
}

/** Steps 1 and 2 done: the tools listed, the user's labels left as they come. */
async function listed(key = "pat-secret-1") {
	const page = await startedAdding(key);
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
