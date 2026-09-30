import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
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
	model: { id: "claude-opus-5", effort: "high" },
});
const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		agent("sol", "Sol", "scrum_master", "scrum-master"),
		agent("ada", "Ada", "architect", "architect"),
		agent("theo", "Theo", "software_developer", "developer"),
		{
			...agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
			model: undefined,
		},
	],
	budgets: {},
	policy: {
		integration: "auto_merge",
		permissions: { run_commands: false, push: false },
	},
	rules: {},
};
const OPUS = {
	id: "claude-opus-5",
	label: "Strongest model, thinks hard (older)",
};
/** What the daemon works out for each agent: here, the team said No to commands. */
const EFFECTIVE = [
	["mira", OPUS, ["read", "network"]],
	["sol", OPUS, ["read"]],
	["ada", OPUS, ["read", "write_workspace", "network", "git_local"]],
	["theo", OPUS, ["read", "write_workspace", "git_local"]],
	[
		"kai",
		{ id: "claude-sonnet-5", label: "Everyday model" },
		["read", "network", "write_workspace", "git_local"],
	],
].map(([id, model, tiers]) => ({
	id,
	model: { ...(model as object), effort: "high" },
	tiers,
	base_tiers: tiers,
}));
const JUDGES = {
	auto: { agent_id: "ada", display_name: "Ada", role: "architect" },
	architect: { agent_id: "ada", display_name: "Ada", role: "architect" },
	scrum_master: { agent_id: "sol", display_name: "Sol", role: "scrum_master" },
};
const MODELS = {
	models: [
		{ id: "claude-opus-5", label: "Strongest model, thinks hard" },
		{ id: "claude-sonnet-5", label: "Everyday model" },
	],
};

/** Opens `path` with the team and the models answered. */
async function opened(path: string) {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerQuery(s, "team.get", {
		team: TEAM,
		agents: EFFECTIVE,
		judges: JUDGES,
	});
	await answerQuery(s, "models.list", MODELS);
	return { container, s };
}

/** The latest request of `method` the page sent. */
async function sent(socket: FakeSocket, method: string) {
	return waitFor(() => {
		const frame = socket.calls(method).at(-1);
		if (!frame) throw new Error(`no ${method} was sent`);
		return frame;
	});
}

/** The latest `team.validate` the page asked, answered with `effects`. */
async function validated(s: FakeSocket, effects: string[]) {
	const frame = await waitFor(() => {
		const f = s
			.calls("query")
			.filter((q) => q.params.name === "team.validate")
			.at(-1);
		if (!f) throw new Error("no team.validate was asked");
		return f;
	});
	act(() => s.reply(frame, { errors: [], effects }));
	return (frame.params.params as { team: typeof TEAM }).team;
}

/** The latest command the page sent, already sent. */
const sent_ = (s: FakeSocket) => s.calls("command").at(-1) as never;

const saved = async (s: FakeSocket) =>
	((await sent(s, "team.save")).params as { team: typeof TEAM }).team;
const one = (team: typeof TEAM, id: string) =>
	team.agents.find((a) => a.id === id) as Record<string, unknown>;

describe("team page", () => {
	afterEach(() => localStorage.clear());

	it("lists_the_team_with_the_first_day_line", async () => {
		const { container, s } = await opened("/team");
		const list = await screen.findByRole("list", { name: en.teamMembers });
		expect(within(list).getAllByRole("listitem")).toHaveLength(5);
		expect(within(list).getByText("Ada persona")).toBeTruthy();
		// Every agent's model in words, its own or its role's, never an id.
		expect(
			within(list).getAllByText("Strongest model, thinks hard (older)"),
		).toHaveLength(4);
		expect(within(list).getByText("Everyday model")).toBeTruthy();
		expect(within(list).queryByText(/claude-/)).toBeNull();
		expect(
			within(list)
				.getByRole("link", { name: "Edit Theo" })
				.getAttribute("href"),
		).toBe("/team/theo");
		expect(screen.getByText(en.firstDay, { exact: false })).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("button", { name: "Pause Theo" }));
		expect((await sent(s, "command")).params).toEqual({
			command: {
				command: "agent_update",
				body: { agent_id: "theo", status: "paused" },
			},
		});
	});

	it("adds_someone_to_the_team", async () => {
		const { s } = await opened("/team");
		fireEvent.change(await screen.findByLabelText(en.teamAddRole), {
			target: { value: "software_developer" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		const team = await saved(s);
		expect(team.agents).toHaveLength(6);
		expect(team.agents.at(-1)).toMatchObject({
			id: "noor",
			display_name: "Noor",
			role: "software_developer",
			status: "active",
		});
		cleanup();

		// Seven not retired is a full team: Add someone says why it is off.
		const full = await renderApp("/team");
		const f = full.socket as FakeSocket;
		const seven = {
			...TEAM,
			agents: [
				...TEAM.agents,
				agent("noor", "Noor", "software_developer", "extra-1"),
				agent("ivo", "Ivo", "software_developer", "extra-2"),
				{ ...agent("lena", "Lena", "architect", "extra-3"), status: "retired" },
			],
		};
		await answerQuery(f, "team.get", {
			team: seven,
			agents: EFFECTIVE,
			judges: JUDGES,
		});
		const add = (await screen.findByRole("button", {
			name: en.teamAdd,
		})) as HTMLButtonElement;
		expect(add.disabled).toBe(true);
		expect(screen.getByText(en.teamFull)).toBeTruthy();
	});

	it("says_a_refused_pause_in_plain_words", async () => {
		const { s } = await opened("/team");
		fireEvent.click(await screen.findByRole("button", { name: "Pause Theo" }));
		act(() =>
			s.reply(sent_(s), {
				error: {
					kind: "refused",
					detail:
						"last_of_role: Theo is your only Software Developer; add another before Theo stops.",
				},
			}),
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			"Theo is your only Developer, so add another Developer first.",
		);
	});

	it("edits_an_agent_and_shows_the_effect_first", async () => {
		const { container, s } = await opened("/team/theo");
		expect(
			await screen.findByRole("heading", { name: "Theo, your Developer" }),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("radio", { name: /^Quick/ }));
		const checked = await validated(s, ["Theo now thinks with low effort."]);
		expect(one(checked, "theo").model).toEqual({
			id: "claude-opus-5",
			effort: "low",
		});
		expect(
			await screen.findByText("Theo now thinks with low effort."),
		).toBeTruthy();
		expect(s.calls("team.save")).toHaveLength(0);

		fireEvent.click(screen.getByRole("button", { name: en.agentSave }));
		expect(one(await saved(s), "theo").model).toEqual({
			id: "claude-opus-5",
			effort: "low",
		});
	});

	it("changes_effort_without_changing_the_model", async () => {
		const { s } = await opened("/team/kai");
		await screen.findByRole("heading", {
			name: "Kai, your Marketing Specialist",
		});
		fireEvent.click(screen.getByRole("radio", { name: /^Quick/ }));
		const checked = await validated(s, ["Kai now thinks with low effort."]);
		expect(one(checked, "kai").model).toEqual({ effort: "low" });
	});

	it("toggles_tiers_in_advanced", async () => {
		const { container, s } = await opened("/team/ada");
		await screen.findByRole("heading", { name: "Ada, your Architect" });
		expect(screen.queryByRole("switch", { name: en.tierNetwork })).toBeNull();

		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		const tiers = screen.getByRole("list", { name: "What Ada may do" });
		expect(within(tiers).getAllByRole("switch")).toHaveLength(7);
		const network = within(tiers).getByRole("switch", {
			name: en.tierNetwork,
		});
		expect(network.getAttribute("aria-checked")).toBe("true");
		const remote = within(tiers).getByRole("switch", {
			name: en.tierGitRemote,
		});
		expect(remote.getAttribute("aria-checked")).toBe("false");
		// The daemon's answer, not the role's defaults: this team said No to commands.
		expect(
			within(tiers)
				.getByRole("switch", { name: en.tierExecute })
				.getAttribute("aria-checked"),
		).toBe("false");
		await expectNoAxeViolations(container);

		fireEvent.click(network);
		expect(network.getAttribute("aria-checked")).toBe("false");
		fireEvent.click(remote);
		expect(remote.getAttribute("aria-checked")).toBe("true");
		await validated(s, ["Ada may no longer use the internet."]);
		fireEvent.click(screen.getByRole("button", { name: en.agentSave }));
		const ada = one(await saved(s), "ada");
		expect(ada.revokes).toEqual(["network"]);
		expect(ada.grants).toEqual(["git_remote"]);
	});

	it("replaces_an_agent", async () => {
		const { s } = await opened("/team/theo");
		fireEvent.click(
			await screen.findByRole("button", { name: "Replace Theo" }),
		);
		// One call: the team is never without its Developer, even for a moment.
		const replace = await sent(s, "agent.replace");
		const { agent_id, newcomer } = replace.params as {
			agent_id: string;
			newcomer: Record<string, unknown>;
		};
		expect(agent_id).toBe("theo");
		expect(newcomer).toMatchObject({
			id: "noor",
			display_name: "Noor",
			role: "software_developer",
			status: "active",
		});
		expect(s.calls("command")).toHaveLength(0);
		expect(s.calls("team.save")).toHaveLength(0);
		act(() => s.reply(replace, {}));
		// The Team page asks for the team again.
		const again = await waitFor(() => {
			const f = s.calls("query").filter((q) => q.params.name === "team.get");
			if (f.length < 2) throw new Error("the team was not asked again");
			return f.at(-1) as never;
		});
		act(() =>
			s.reply(again, { team: TEAM, agents: EFFECTIVE, judges: JUDGES }),
		);
		expect(
			await screen.findByRole("heading", { name: en.teamMembers }),
		).toBeTruthy();
	});

	it("shows_and_disconnects_the_account", async () => {
		const { container, socket } = await renderApp("/settings");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "account.status", {
			provider: "anthropic",
			kind: "api_key",
			source: "keychain",
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		expect(within(row).getByText(/an API key/)).toBeTruthy();
		expect(within(row).getByText(en.accountKeychain)).toBeTruthy();
		await expectNoAxeViolations(container);

		// Disconnecting pauses the team, so it is asked once more first.
		fireEvent.click(
			within(row).getByRole("button", { name: en.accountDisconnect }),
		);
		expect(within(row).getByText(en.accountConfirm)).toBeTruthy();
		expect(s.calls("account.disconnect")).toHaveLength(0);
		fireEvent.click(
			within(row).getByRole("button", { name: en.accountDisconnectYes }),
		);
		const gone = await sent(s, "account.disconnect");
		expect(gone.params).toEqual({});
		act(() => s.reply(gone, { removed_from: ["keychain"], paused: true }));
		expect(await within(row).findByText(en.accountGone)).toBeTruthy();
		// The row reads the account again rather than saying it is still kept.
		const again = await waitFor(() => {
			const asked = s
				.calls("query")
				.filter((q) => q.params.name === "account.status");
			if (asked.length < 2) throw new Error("the account was not asked again");
			return asked.at(-1) as never;
		});
		act(() => s.reply(again, { provider: null, kind: null, source: null }));
		expect(await within(row).findByText(en.accountNone)).toBeTruthy();
		expect(within(row).queryByText(en.accountKeychain)).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("names_the_variable_a_key_from_the_environment_comes_from", async () => {
		const { socket } = await renderApp("/settings");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "account.status", {
			provider: "anthropic",
			kind: "api_key",
			source: "environment",
			environment_variable: "ANTHROPIC_API_KEY",
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		expect(
			await within(row).findByText(
				en.accountKept.replace("{variable}", "ANTHROPIC_API_KEY"),
			),
		).toBeTruthy();
		expect(
			within(row).queryByRole("button", { name: en.accountDisconnect }),
		).toBeNull();
	});
});
