import { AVATAR_URLS } from "@farik/ui";
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
	model: { id: "claude-opus-5-5", effort: "high" },
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
	id: "claude-opus-5-5",
	label: "Strongest model, thinks hard",
};
/** What the daemon works out for each agent: here, the team said No to commands. */
const EFFECTIVE = [
	["mira", OPUS, ["read", "network"]],
	["sol", OPUS, ["read"]],
	["ada", OPUS, ["read", "write_workspace", "network", "git_local"]],
	["theo", OPUS, ["read", "write_workspace", "git_local"]],
	[
		"kai",
		{ id: "claude-sonnet-5-5", label: "Everyday model" },
		["read", "network", "write_workspace", "git_local"],
	],
	["iris", OPUS, ["read", "write_workspace", "execute", "git_local"]],
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
		{ id: "claude-opus-5-5", label: "Strongest model, thinks hard" },
		{ id: "claude-sonnet-5-5", label: "Everyday model" },
	],
};

/** Opens `path` with `team` and the models answered. */
async function opened(path: string, team: object = TEAM) {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", {
		team,
		agents: EFFECTIVE,
		judges: JUDGES,
		max_agents: 7,
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
	await s.reply(frame, { errors: [], effects });
	return (frame.params.params as { team: typeof TEAM }).team;
}

/** The latest command the page sent, already sent. */
const sent_ = (s: FakeSocket) => s.calls("command").at(-1) as never;

const saved = async (s: FakeSocket) =>
	((await sent(s, "team.save")).params as { team: typeof TEAM }).team;
const one = (team: typeof TEAM, id: string) =>
	team.agents.find((a) => a.id === id) as Record<string, unknown>;

const BUDGET = "Does the task fit its budget?";
const NOTICE =
	"Would its checks notice if the work went wrong the way its intent worries about?";
const SMALL = "Is it small enough to finish in one go?";
const UI_PATHS = [
	"**/*.tsx",
	"**/*.jsx",
	"**/*.vue",
	"**/*.svelte",
	"**/*.css",
	"**/*.scss",
	"**/*.html",
];
const PLAYWRIGHT = [{ name: "playwright", source: "builtin" }];
/** The team with Iris, the UI/UX Designer, and her connector. */
const WITH_IRIS = {
	...TEAM,
	agents: [
		...TEAM.agents,
		{
			...agent("iris", "Iris", "ui_ux_designer", "extra-1"),
			mcp_servers: PLAYWRIGHT,
		},
	],
};
/** What `farik init` writes, as settings.defaults answers it. */
const DEFAULTS = {
	budgets: {},
	rules: {},
	policy: {
		integration: "auto_merge",
		judgment: {
			required: "always",
			questions: [BUDGET, NOTICE],
			judge: "auto",
		},
		permissions: { run_commands: true, push: false },
		plan_in_sprints: true,
	},
	ui_paths: UI_PATHS,
};

/** Settings, with the team and the defaults answered: its team's rules area. */
async function settings(team: object = TEAM, path = "/settings") {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	// The shell asks first, then the page itself.
	await answerStatus(s, false);
	await answerStatus(s, false, 2);
	await answerQuery(s, "team.get", {
		team,
		agents: EFFECTIVE,
		judges: JUDGES,
		max_agents: 7,
	});
	await answerQuery(s, "settings.defaults", DEFAULTS);
	const rules = await screen.findByRole("region", { name: en.rulesArea });
	const part = (name: string) =>
		within(rules).getByRole("region", { name }) as HTMLElement;
	return { container, s, rules, part };
}

const picked = (el: HTMLElement) => (el as HTMLInputElement).checked;

describe("team page", () => {
	afterEach(() => localStorage.clear());

	it("lists_the_team_with_the_first_day_line", async () => {
		const { container, s } = await opened("/team");
		const list = await screen.findByRole("list", { name: en.teamMembers });
		expect(within(list).getAllByRole("listitem")).toHaveLength(5);
		expect(within(list).getByText("Ada persona")).toBeTruthy();
		// Every agent's model in words, its own or its role's, never an id.
		expect(
			within(list).getAllByText("Strongest model, thinks hard"),
		).toHaveLength(4);
		expect(within(list).getByText("Everyday model")).toBeTruthy();
		expect(within(list).queryByText(/claude-/)).toBeNull();
		expect(
			within(list)
				.getByRole("link", { name: "Edit Theo" })
				.getAttribute("href"),
		).toBe("/team/theo");
		expect(screen.getByText(en.firstDay, { exact: false })).toBeTruthy();
		// The figure is the suggested six's; a team of another size is told how it compares.
		expect(en.firstDay).toMatch(/A smaller team costs less\.$/);
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("button", { name: "Pause Theo" }));
		expect((await sent(s, "command")).params).toEqual({
			command: {
				command: "agent_update",
				body: { agent_id: "theo", status: "paused" },
			},
		});
	});

	it("counts_no_agents_in_the_designer_cost_line_of_a_small_team", async () => {
		await opened("/team", {
			...TEAM,
			agents: [
				agent("mira", "Mira", "product_manager", "product_manager"),
				agent("ada", "Ada", "architect", "architect"),
				agent("theo", "Theo", "software_developer", "software_developer"),
				agent("iris", "Iris", "ui_ux_designer", "extra-1"),
			],
		});
		const cost = await screen.findByRole("region", { name: en.teamCostTitle });
		// A team of four has no sixth agent.
		expect(cost.textContent).not.toMatch(/sixth/);
		expect(
			within(cost).getByText("so Iris adds to what a day costs", {
				exact: false,
			}),
		).toBeTruthy();
	});

	it("shows_the_designer_card_in_its_colour", async () => {
		const { container, s } = await opened("/team", {
			...TEAM,
			agents: [
				...TEAM.agents.slice(0, 4),
				agent("iris", "Iris", "ui_ux_designer", "extra-1"),
				...TEAM.agents.slice(4),
			],
		});
		const list = await screen.findByRole("list", { name: en.teamMembers });
		const card = within(list)
			.getByText("Iris persona")
			.closest("li") as HTMLElement;
		const tag = within(card).getByTitle("UI/UX Designer");
		expect(tag.textContent).toBe("UX");
		expect(tag.className).toMatch(/uiUxDesigner/);
		const face = card.querySelector("img") as HTMLImageElement;
		expect(face.getAttribute("src")).toBe(AVATAR_URLS["extra-1"]);
		// Each card says what its agent is doing, in the team's activity words.
		await answerQuery(s, "team.activity", {
			activity: [
				{
					agent_id: "iris",
					state: "idle",
					line: "Waiting for Mira to approve a plan",
					task_id: "FRK-21",
				},
			],
		});
		expect(
			await within(card).findByText("Waiting for Mira to approve a plan"),
		).toBeTruthy();
		expect(within(card).queryByText(en.agentActive)).toBeNull();
		expect(face.style.getPropertyValue("--ring")).toBe(
			"var(--farik-color-role-ui-ux-designer)",
		);
		// The cost section says what the Designer, and its plans, add.
		expect(
			screen.getByText(
				"Iris uses the same model as Theo, so Iris adds to what a day costs. Before Iris changes a screen, Iris looks at it and writes a plan, and that costs a little too.",
				{ exact: false },
			),
		).toBeTruthy();
		const cost = screen.getByRole("region", { name: en.teamCostTitle });
		expect(
			within(cost).getByRole("link", { name: "Costs" }).getAttribute("href"),
		).toBe("/costs");
		await expectNoAxeViolations(container);

		// Add someone offers the Designer, with a picture nobody on the team has.
		fireEvent.change(screen.getByLabelText(en.teamAddRole), {
			target: { value: "ui_ux_designer" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		const team = await saved(s);
		expect(team.agents.at(-1)).toMatchObject({
			display_name: "Noor",
			role: "ui_ux_designer",
			avatar: "extra-2",
		});
	});

	it("the_team_page_adds_finance", async () => {
		const { s } = await opened("/team");
		const select = (await screen.findByLabelText(
			en.teamAddRole,
		)) as HTMLSelectElement;
		// The Finance Specialist is offered after the Marketing Specialist, and is not suggested.
		expect(
			within(select)
				.getAllByRole("option")
				.map((option) => option.textContent),
		).toEqual([
			en.roleDeveloper,
			en.roleProductManager,
			en.roleScrumMaster,
			en.roleArchitect,
			en.roleDesigner,
			en.roleMarketing,
			"Finance Specialist",
		]);
		fireEvent.change(select, { target: { value: "finance_specialist" } });
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		const team = await saved(s);
		expect(team.agents.at(-1)).toMatchObject({
			display_name: "Noor",
			role: "finance_specialist",
			avatar: "finance-specialist",
			status: "active",
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

		// As many not retired as the daemon allows is a full team: Add someone says why it is off.
		const full = await renderApp("/team");
		const f = full.socket as FakeSocket;
		await answerStatus(f, false);
		const six = {
			...TEAM,
			agents: [
				...TEAM.agents,
				agent("noor", "Noor", "software_developer", "extra-1"),
				{ ...agent("lena", "Lena", "architect", "extra-3"), status: "retired" },
			],
		};
		await answerQuery(f, "team.get", {
			team: six,
			agents: EFFECTIVE,
			judges: JUDGES,
			max_agents: 6,
		});
		const add = (await screen.findByRole("button", {
			name: en.teamAdd,
		})) as HTMLButtonElement;
		expect(add.disabled).toBe(true);
		expect(screen.getByText(en.teamFull)).toBeTruthy();
	});

	it("resumes_a_paused_agent_from_its_card", async () => {
		const { container, socket } = await renderApp("/team");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		const paused = {
			...TEAM,
			agents: TEAM.agents.map((a) =>
				a.id === "theo" ? { ...a, status: "paused" } : a,
			),
		};
		await answerQuery(s, "team.get", {
			team: paused,
			agents: EFFECTIVE,
			judges: JUDGES,
		});
		fireEvent.click(await screen.findByRole("button", { name: "Resume Theo" }));
		expect((await sent(s, "command")).params).toEqual({
			command: {
				command: "agent_update",
				body: { agent_id: "theo", status: "active" },
			},
		});
		expect(screen.getByRole("button", { name: "Pause Ada" })).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("offers_save_only_once_something_changed", async () => {
		await opened("/team/theo");
		const save = (await screen.findByRole("button", {
			name: en.agentSave,
		})) as HTMLButtonElement;
		expect(save.disabled).toBe(true);
		fireEvent.click(screen.getByRole("radio", { name: /^Quick/ }));
		expect(save.disabled).toBe(false);
		fireEvent.click(screen.getByRole("button", { name: en.agentCancel }));
		expect(save.disabled).toBe(true);
	});

	it("says_a_refused_pause_in_plain_words", async () => {
		const { s } = await opened("/team");
		fireEvent.click(await screen.findByRole("button", { name: "Pause Theo" }));
		await s.reply(sent_(s), {
			error: {
				kind: "refused",
				detail:
					"last_of_role: Theo is your only Software Developer; add another before Theo stops.",
			},
		});
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
		const checked = await validated(s, ["Theo now works quickly."]);
		expect(one(checked, "theo").model).toEqual({
			id: "claude-opus-5-5",
			effort: "low",
		});
		expect(await screen.findByText("Theo now works quickly.")).toBeTruthy();
		expect(s.calls("team.save")).toHaveLength(0);

		fireEvent.click(screen.getByRole("button", { name: en.agentSave }));
		expect(one(await saved(s), "theo").model).toEqual({
			id: "claude-opus-5-5",
			effort: "low",
		});
	});

	it("changes_effort_without_changing_the_model", async () => {
		const { s } = await opened("/team/kai");
		await screen.findByRole("heading", {
			name: "Kai, your Marketing Specialist",
		});
		fireEvent.click(screen.getByRole("radio", { name: /^Quick/ }));
		const checked = await validated(s, ["Kai now works quickly."]);
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
		await s.reply(replace, {});
		// The Team page asks for the team again.
		const again = await waitFor(() => {
			const f = s.calls("query").filter((q) => q.params.name === "team.get");
			if (f.length < 2) throw new Error("the team was not asked again");
			return f.at(-1) as never;
		});
		await s.reply(again, { team: TEAM, agents: EFFECTIVE, judges: JUDGES });
		expect(
			await screen.findByRole("heading", { name: en.teamMembers }),
		).toBeTruthy();
	});

	it("says_a_failed_save_replace_or_add_in_plain_words", async () => {
		const RAW = "/agents/3 has additional properties";
		/** The page's alert, once `method`'s call failed with raw words. */
		const failed = async (s: FakeSocket, method: string) => {
			await s.fail(await sent(s, method), -32602, RAW);
			const alert = await screen.findByRole("alert");
			expect(alert.textContent).toContain(en.refuseOther);
			expect(screen.queryByText(new RegExp(RAW))).toBeNull();
		};

		const edit = await opened("/team/theo");
		fireEvent.click(await screen.findByRole("radio", { name: /^Quick/ }));
		await validated(edit.s, []);
		fireEvent.click(screen.getByRole("button", { name: en.agentSave }));
		await sent(edit.s, "team.save");
		await failed(edit.s, "team.save");
		fireEvent.click(screen.getByRole("button", { name: "Replace Theo" }));
		await sent(edit.s, "agent.replace");
		await failed(edit.s, "agent.replace");
		cleanup();

		const team = await opened("/team");
		fireEvent.click(await screen.findByRole("button", { name: en.teamAdd }));
		await sent(team.s, "team.save");
		await failed(team.s, "team.save");
	});

	it("says_a_failed_disconnect_in_plain_words", async () => {
		const { socket } = await renderApp("/settings");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "account.status", {
			provider: "anthropic",
			kind: "api_key",
			source: "keychain",
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		fireEvent.click(
			within(row).getByRole("button", { name: en.accountDisconnect }),
		);
		fireEvent.click(
			within(row).getByRole("button", { name: en.accountDisconnectYes }),
		);
		const gone = await sent(s, "account.disconnect");
		await s.fail(gone, -32603, "keyring: platform secure storage failure");
		expect(await within(row).findByText(en.refuseOther)).toBeTruthy();
		expect(within(row).queryByText(/keyring/)).toBeNull();
	});

	it("switches_a_connector", async () => {
		const { container, s } = await opened("/team/theo");
		await screen.findByRole("heading", { name: "Theo, your Developer" });
		const connectors = screen.getByRole("region", { name: /^Connectors/ });
		const playwright = within(connectors).getByRole("switch", {
			name: en.connectorPlaywright,
		});
		expect(playwright.getAttribute("aria-checked")).toBe("false");
		await expectNoAxeViolations(container);

		fireEvent.click(playwright);
		expect(playwright.getAttribute("aria-checked")).toBe("true");
		const effect = "Theo may now open your app in a browser.";
		const checked = await validated(s, [effect]);
		expect(one(checked, "theo").mcp_servers).toEqual(PLAYWRIGHT);
		// What changes is shown before anything is saved.
		expect(await screen.findByText(effect)).toBeTruthy();
		expect(s.calls("team.save")).toHaveLength(0);
		fireEvent.click(screen.getByRole("button", { name: en.agentSave }));
		expect(one(await saved(s), "theo").mcp_servers).toEqual(PLAYWRIGHT);
		cleanup();

		// Turned off for the Designer, the page says what that costs.
		await opened("/team/iris", WITH_IRIS);
		await screen.findByRole("heading", { name: "Iris, your UI/UX Designer" });
		const hers = screen.getByRole("switch", { name: en.connectorPlaywright });
		expect(hers.getAttribute("aria-checked")).toBe("true");
		expect(
			screen.queryByText(
				"Without it Iris cannot look at your app, so Farik gives Iris no work.",
			),
		).toBeNull();
		fireEvent.click(hers);
		expect(
			screen.getByText(
				"Without it Iris cannot look at your app, so Farik gives Iris no work.",
			),
		).toBeTruthy();
	});

	it("saves_the_preview_in_settings", async () => {
		const scrolled = vi.fn();
		Element.prototype.scrollIntoView = scrolled;
		const { container, s } = await settings(WITH_IRIS, "/settings#preview");
		const preview = await screen.findByRole("region", {
			name: en.previewTitle,
		});
		expect(preview.id).toBe("preview");
		// Today's row links here, and the page goes to the section.
		await waitFor(() => expect(scrolled.mock.contexts).toContain(preview));
		expect(
			within(preview).getByText(
				"Iris, your UI/UX Designer, looks at your app in a browser to check its screens. Tell Farik the commands you use to open it. Farik runs them inside Docker’s sandbox, never straight on your computer.",
			),
		).toBeTruthy();
		expect(within(preview).getByText(en.previewNotSure)).toBeTruthy();
		await expectNoAxeViolations(container);

		const field = (name: string) =>
			within(preview).getByRole("textbox", { name });
		fireEvent.change(field(en.previewPrepare), {
			target: { value: "pnpm install" },
		});
		fireEvent.change(field(en.previewStart), {
			target: { value: "pnpm dev" },
		});
		fireEvent.change(field(en.previewPort), { target: { value: "5173" } });
		const effect = "Iris can now open your app.";
		const draft = (await validated(s, [effect])) as unknown as {
			preview: unknown;
		};
		const expected = {
			prepare: "pnpm install",
			start: "pnpm dev",
			port: 5173,
			path: "/",
		};
		expect(draft.preview).toEqual(expected);
		expect(await within(preview).findByText(effect)).toBeTruthy();
		expect(s.calls("team.save")).toHaveLength(0);
		fireEvent.click(
			within(preview).getByRole("button", { name: en.agentSave }),
		);
		expect(
			((await saved(s)) as unknown as { preview: unknown }).preview,
		).toEqual(expected);
		cleanup();

		// A team without a Designer is never asked.
		await settings();
		expect(screen.queryByRole("region", { name: en.previewTitle })).toBeNull();
	});

	it("edits_the_ui_paths_in_advanced", async () => {
		const { container, s } = await settings(WITH_IRIS);
		expect(screen.queryByRole("region", { name: en.uiPathsTitle })).toBeNull();
		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		const paths = screen.getByRole("region", { name: en.uiPathsTitle });
		const list = within(paths).getByRole("list");
		// The team says none, so the seven defaults stand.
		expect(
			within(list)
				.getAllByRole("listitem")
				.map((li) => li.querySelector("code")?.textContent),
		).toEqual(UI_PATHS);
		await expectNoAxeViolations(container);

		fireEvent.change(
			within(paths).getByRole("textbox", { name: en.uiPathsAdd }),
			{
				target: { value: "**/*.strings" },
			},
		);
		fireEvent.click(
			within(paths).getByRole("button", { name: en.uiPathsAddButton }),
		);
		const effect = "Iris checks 8 kinds of file.";
		const draft = await validated(s, [effect]);
		expect(draft.rules).toEqual({ ui_paths: [...UI_PATHS, "**/*.strings"] });
		expect(await within(paths).findByText(effect)).toBeTruthy();
		fireEvent.click(within(paths).getByRole("button", { name: en.agentSave }));
		expect((await saved(s)).rules).toEqual({
			ui_paths: [...UI_PATHS, "**/*.strings"],
		});

		// Each one can go, by name.
		fireEvent.click(
			within(paths).getByRole("button", { name: "Remove **/*.html" }),
		);
		const fewer = await validated(s, []);
		expect((fewer.rules as { ui_paths: string[] }).ui_paths).not.toContain(
			"**/*.html",
		);
	});

	it("changes_what_agents_may_do_and_the_limit_from_settings", async () => {
		const { container, s, rules, part } = await settings();
		expect(within(rules).getByText(en.rulesNextSession)).toBeTruthy();
		await expectNoAxeViolations(container);

		// The team's own answers, not setup's blanks.
		const may = part(en.rulesMay);
		expect(
			picked(within(may).getByRole("radio", { name: /^No, nobody may/ })),
		).toBe(true);
		const save = within(may).getByRole("button", {
			name: en.agentSave,
		}) as HTMLButtonElement;
		expect(save.disabled).toBe(true);
		fireEvent.click(
			within(may).getByRole("radio", { name: /^Yes, on its own branches/ }),
		);
		const effect = "Developers may now push their work and open pull requests.";
		const draft = await validated(s, [effect]);
		expect(draft.policy.permissions).toEqual({
			run_commands: false,
			push: true,
		});
		// What changes is shown before anything is saved.
		expect(await within(may).findByText(effect)).toBeTruthy();
		expect(s.calls("team.save")).toHaveLength(0);
		fireEvent.click(save);
		const first = await sent(s, "team.save");
		expect(
			(first.params as { team: typeof TEAM }).team.policy.permissions,
		).toEqual({ run_commands: false, push: true });
		// A failed save is said in plain words, never the daemon's own text.
		await s.fail(
			first,
			-32602,
			"/policy/permissions has additional properties",
		);
		expect((await within(may).findByRole("alert")).textContent).toBe(
			en.refuseOther,
		);

		// Put back the default: the answers `farik init` writes.
		fireEvent.click(within(may).getByRole("button", { name: en.putBack }));
		expect(
			picked(
				within(may).getByRole("radio", {
					name: /^Yes, the Developer and Architect may/,
				}),
			),
		).toBe(true);
		expect(
			picked(within(may).getByRole("radio", { name: /^No, keep everything/ })),
		).toBe(true);
		fireEvent.click(within(may).getByRole("button", { name: en.agentSave }));
		await waitFor(() => expect(s.calls("team.save")).toHaveLength(2));
		await s.reply(s.calls("team.save")[1] as never, {});
		// Saved: the page reads the team again.
		await waitFor(() =>
			expect(
				s.calls("query").filter((q) => q.params.name === "team.get").length,
			).toBeGreaterThan(1),
		);

		// The same daily limit as the Costs page's.
		const spend = part(en.rulesSpendTitle);
		expect(
			picked(within(spend).getByRole("radio", { name: /^No limit/ })),
		).toBe(true);
		fireEvent.click(
			within(spend).getByRole("radio", {
				name: new RegExp(`^${en.spendDaily}`),
			}),
		);
		fireEvent.change(within(spend).getByLabelText(en.spendAmount), {
			target: { value: "15" },
		});
		const limited = await validated(s, ["The team may spend up to $15 a day."]);
		expect(limited.budgets).toEqual({ daily_usd: 15 });
		expect(
			await within(spend).findByText("The team may spend up to $15 a day."),
		).toBeTruthy();
		fireEvent.click(within(spend).getByRole("button", { name: en.agentSave }));
		await waitFor(() => expect(s.calls("team.save")).toHaveLength(3));
		expect((await saved(s)).budgets).toEqual({ daily_usd: 15 });
	});

	it("changes_how_work_is_added_and_how_plans_are_checked_from_settings", async () => {
		const { container, s, part } = await settings();

		const finish = part(en.rulesFinish);
		expect(
			picked(
				within(finish).getByRole("radio", {
					name: new RegExp(`^${en.finishAuto}`),
				}),
			),
		).toBe(true);
		fireEvent.click(
			within(finish).getByRole("radio", {
				name: new RegExp(`^${en.finishManual}`),
			}),
		);
		const manual = await validated(s, [
			"Finished work waits for you to merge it.",
		]);
		expect(manual.policy.integration).toBe("manual");
		expect(
			await within(finish).findByText(
				"Finished work waits for you to merge it.",
			),
		).toBeTruthy();
		fireEvent.click(within(finish).getByRole("button", { name: en.putBack }));
		expect(
			picked(
				within(finish).getByRole("radio", {
					name: new RegExp(`^${en.finishAuto}`),
				}),
			),
		).toBe(true);
		expect(
			(
				within(finish).getByRole("button", {
					name: en.agentSave,
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);

		const plans = part(en.planTitle);
		const small = within(plans).getByRole("checkbox", {
			name: new RegExp(`^${en.planSmall}`),
		});
		expect(picked(small)).toBe(false);
		fireEvent.click(small);
		fireEvent.click(within(plans).getByRole("radio", { name: /Scrum Master/ }));
		const checked = await validated(s, [
			"Plans are checked against 3 questions.",
			"The Scrum Master checks every plan.",
		]);
		expect(checked.policy).toMatchObject({
			judgment: {
				required: "always",
				questions: [BUDGET, NOTICE, SMALL],
				judge: "scrum_master",
			},
		});
		expect(
			await within(plans).findByText("The Scrum Master checks every plan."),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		// A plan check with no questions is refused at the questions, in plain words.
		for (const q of [
			within(plans).getByRole("checkbox", {
				name: new RegExp(`^${en.planBudget}`),
			}),
			within(plans).getByRole("checkbox", {
				name: new RegExp(`^${en.planNotice}`),
			}),
			small,
		])
			fireEvent.click(q);
		const frame = await waitFor(() => {
			const f = s
				.calls("query")
				.filter((q) => q.params.name === "team.validate")
				.at(-1);
			const team = (f?.params.params as { team: typeof TEAM } | undefined)
				?.team as
				| { policy: { judgment?: { questions: string[] } } }
				| undefined;
			if (team?.policy.judgment?.questions.length !== 0)
				throw new Error("the empty check was not asked");
			return f as never;
		});
		await s.reply(frame, {
			errors: [
				{
					path: "/policy/judgment/questions",
					message: "/policy/judgment/questions has fewer than 1 items",
					code: "no_questions",
				},
			],
			effects: [],
		});
		expect(await within(plans).findByText(en.refuseNoQuestions)).toBeTruthy();
		expect(within(plans).queryByText(/fewer than/)).toBeNull();
		expect(
			(
				within(plans).getByRole("button", {
					name: en.agentSave,
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);

		fireEvent.click(within(plans).getByRole("button", { name: en.putBack }));
		fireEvent.click(small);
		await validated(s, ["Plans are checked against 3 questions."]);
		expect(
			await within(plans).findByText("Plans are checked against 3 questions."),
		).toBeTruthy();
		fireEvent.click(within(plans).getByRole("button", { name: en.agentSave }));
		const saved_ = await saved(s);
		expect(saved_.policy).toMatchObject({
			judgment: {
				required: "always",
				questions: [BUDGET, NOTICE, SMALL],
				judge: "auto",
			},
		});
	});

	it("reads_an_old_team_as_not_planning_in_sprints", async () => {
		// TEAM has no plan_in_sprints, as a team file from before step 15; the default is on.
		expect("plan_in_sprints" in TEAM.policy).toBe(false);
		const { part } = await settings(TEAM);
		const toggle = within(part(en.rulesPlanning)).getByRole("switch", {
			name: en.planInSprints,
		});
		expect(toggle.getAttribute("aria-checked")).toBe("false");
	});

	it("switches_planning_in_sprints", async () => {
		const on = { ...TEAM, policy: { ...TEAM.policy, plan_in_sprints: true } };
		const { container, s, rules, part } = await settings(on);
		// The section sits after "How finished work is added", before the plan check.
		const headings = within(rules)
			.getAllByRole("heading", { level: 2 })
			.map((h) => h.textContent);
		expect(headings.indexOf(en.rulesPlanning)).toBe(
			headings.indexOf(en.rulesFinish) + 1,
		);
		const planning = part(en.rulesPlanning);
		const toggle = within(planning).getByRole("switch", {
			name: en.planInSprints,
		});
		expect(toggle.getAttribute("aria-checked")).toBe("true");
		expect(toggle.getAttribute("aria-describedby")).toBeTruthy();
		expect(within(planning).getByText(en.planInSprintsNote)).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(toggle);
		expect(toggle.getAttribute("aria-checked")).toBe("false");
		const effects = [
			"Ready work starts as soon as someone is free, without waiting for a sprint.",
			"The 2 pieces of work in the Backlog, Gift cards at checkout and Sold-out badge on the menu, can start now.",
			"You can still start sprints from the Board.",
		];
		const off = await validated(s, effects);
		expect(off.policy).toMatchObject({ plan_in_sprints: false });
		for (const line of effects)
			expect(await within(planning).findByText(line)).toBeTruthy();
		await expectNoAxeViolations(container);

		// "Put back the default" turns it on again: nothing to save.
		fireEvent.click(within(planning).getByRole("button", { name: en.putBack }));
		expect(toggle.getAttribute("aria-checked")).toBe("true");
		const save = () =>
			within(planning).getByRole("button", {
				name: en.agentSave,
			}) as HTMLButtonElement;
		expect(save().disabled).toBe(true);

		fireEvent.click(toggle);
		await validated(s, effects);
		await waitFor(() => expect(save().disabled).toBe(false));
		fireEvent.click(save());
		expect((await saved(s)).policy).toMatchObject({
			integration: "auto_merge",
			plan_in_sprints: false,
		});
	});

	it("keeps_each_rules_draft_until_it_is_saved_or_cancelled", async () => {
		const { s, part } = await settings();
		// The saved team is checked once on load, not once for each part.
		await act(async () => {});
		expect(
			s.calls("query").filter((q) => q.params.name === "team.validate"),
		).toHaveLength(1);

		// A draft in one part survives another part's save and the page's re-read.
		const branches = () =>
			within(part(en.rulesMay)).getByRole("radio", {
				name: /^Yes, on its own branches/,
			});
		fireEvent.click(branches());
		const finish = part(en.rulesFinish);
		fireEvent.click(
			within(finish).getByRole("radio", {
				name: new RegExp(`^${en.finishManual}`),
			}),
		);
		await validated(s, ["Finished work waits for you to merge it."]);
		fireEvent.click(within(finish).getByRole("button", { name: en.agentSave }));
		await s.reply(await sent(s, "team.save"), {});
		const reread = await waitFor(() => {
			const f = s
				.calls("query")
				.filter((q) => q.params.name === "team.get")
				.at(-1);
			if (f === s.calls("query").find((q) => q.params.name === "team.get"))
				throw new Error("the team was not read again");
			return f as never;
		});
		await s.reply(reread, {
			team: { ...TEAM, policy: { ...TEAM.policy, integration: "manual" } },
			agents: EFFECTIVE,
			judges: JUDGES,
			max_agents: 7,
		});
		await waitFor(() =>
			expect(
				picked(
					within(part(en.rulesFinish)).getByRole("radio", {
						name: new RegExp(`^${en.finishManual}`),
					}),
				),
			).toBe(true),
		);
		expect(picked(branches())).toBe(true);

		// Cancel drops the draft: the part shows the team's own answer again.
		fireEvent.click(
			within(part(en.rulesMay)).getByRole("button", { name: en.agentCancel }),
		);
		await waitFor(() => expect(picked(branches())).toBe(false));
		expect(
			picked(
				within(part(en.rulesMay)).getByRole("radio", {
					name: /^No, nobody may/,
				}),
			),
		).toBe(true);

		// A saved limit is what the part shows once the team is read again.
		const spend = () => part(en.rulesSpendTitle);
		fireEvent.click(
			within(spend()).getByRole("radio", {
				name: new RegExp(`^${en.spendDaily}`),
			}),
		);
		fireEvent.change(within(spend()).getByLabelText(en.spendAmount), {
			target: { value: "15" },
		});
		await validated(s, ["The team may spend up to $15 a day."]);
		const reads = s
			.calls("query")
			.filter((q) => q.params.name === "team.get").length;
		fireEvent.click(
			within(spend()).getByRole("button", { name: en.agentSave }),
		);
		await waitFor(() => expect(s.calls("team.save")).toHaveLength(2));
		await s.reply(s.calls("team.save")[1] as never, {});
		const again = await waitFor(() => {
			const all = s.calls("query").filter((q) => q.params.name === "team.get");
			if (all.length === reads) throw new Error("the team was not read again");
			return all.at(-1) as never;
		});
		await s.reply(again, {
			team: { ...TEAM, budgets: { daily_usd: 15 } },
			agents: EFFECTIVE,
			judges: JUDGES,
			max_agents: 7,
		});
		await waitFor(() =>
			expect(
				(within(spend()).getByLabelText(en.spendAmount) as HTMLInputElement)
					.value,
			).toBe("15"),
		);
		expect(
			(
				within(spend()).getByRole("button", {
					name: en.agentSave,
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);

		// An amount that is not above zero is never saved, whatever the check says.
		fireEvent.change(within(spend()).getByLabelText(en.spendAmount), {
			target: { value: "0" },
		});
		await validated(s, ["The team may spend up to $0 a day."]);
		expect(
			(
				within(spend()).getByRole("button", {
					name: en.agentSave,
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);
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
		// The row is there before the account's answer is drawn in it.
		expect(await within(row).findByText(/an API key/)).toBeTruthy();
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
		await s.reply(gone, { removed_from: ["keychain"], paused: true });
		expect(await within(row).findByText(en.accountGone)).toBeTruthy();
		// The row reads the account again rather than saying it is still kept.
		const again = await waitFor(() => {
			const asked = s
				.calls("query")
				.filter((q) => q.params.name === "account.status");
			if (asked.length < 2) throw new Error("the account was not asked again");
			return asked.at(-1) as never;
		});
		await s.reply(again, { provider: null, kind: null, source: null });
		expect(await within(row).findByText(en.accountNone)).toBeTruthy();
		expect(within(row).queryByText(en.accountKeychain)).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("says_the_key_did_not_work_and_connects_again", async () => {
		const { container, socket } = await renderApp("/settings");
		const s = socket as FakeSocket;
		await answerStatus(s, true);
		await answerQuery(s, "account.status", {
			provider: "anthropic",
			kind: "subscription_token",
			source: "keychain",
			key_refused: true,
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		expect(await within(row).findByText(en.accountKeyRefused)).toBeTruthy();
		await expectNoAxeViolations(container);
		const button = within(row).getByRole("button", {
			name: en.accountConnectAgain,
		}) as HTMLButtonElement;
		// Nothing to send until a key is typed.
		expect(button.disabled).toBe(true);
		fireEvent.change(within(row).getByLabelText(en.subscriptionKey), {
			target: { value: "sk-ant-api-wrong" },
		});
		fireEvent.click(button);
		// The daemon's refusal is said in plain words, never its own.
		const wrong = await sent(s, "account.connect");
		await s.fail(
			wrong,
			-32005,
			"that is not a Claude subscription token: a subscription token starts with sk-ant-oat",
		);
		expect(await within(row).findByText(en.setupNotSubscription)).toBeTruthy();
		fireEvent.change(within(row).getByLabelText(en.subscriptionKey), {
			target: { value: "sk-ant-oat01-new" },
		});
		fireEvent.click(button);
		const connect = await waitFor(() => {
			const all = s.calls("account.connect");
			const second = all[1];
			if (!second) throw new Error("no second account.connect");
			return second;
		});
		expect(connect.params).toEqual({
			kind: "subscription_token",
			secret: "sk-ant-oat01-new",
		});
		await s.reply(connect, { stored_in: "keychain", taking_on: false });
		// The row reads the account again, and the key works now.
		const again = await waitFor(() => {
			const asked = s
				.calls("query")
				.filter((q) => q.params.name === "account.status");
			if (asked.length < 2) throw new Error("the account was not asked again");
			return asked.at(-1) as never;
		});
		await s.reply(again, {
			provider: "anthropic",
			kind: "subscription_token",
			source: "keychain",
		});
		await waitFor(() =>
			expect(within(row).queryByText(en.accountKeyRefused)).toBeNull(),
		);
		expect(within(row).getByText(/your Claude subscription/)).toBeTruthy();
	});

	it("says_a_refused_key_from_the_environment_is_changed_where_farik_runs", async () => {
		const { socket } = await renderApp("/settings");
		const s = socket as FakeSocket;
		await answerStatus(s, true);
		await answerQuery(s, "account.status", {
			provider: "anthropic",
			kind: "api_key",
			source: "environment",
			environment_variable: "ANTHROPIC_API_KEY",
			key_refused: true,
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		expect(
			await within(row).findByText(
				en.accountKeyRefusedEnvironment.replace(
					"{variable}",
					"ANTHROPIC_API_KEY",
				),
			),
		).toBeTruthy();
		expect(
			within(row).queryByRole("button", { name: en.accountConnectAgain }),
		).toBeNull();
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

const PAIR = {
	slug: "pair",
	template: {
		version: 1,
		name: "Pair",
		saved_at: "2026-09-28T12:00:00Z",
		agents: [
			{
				id: "mira",
				display_name: "Mira",
				role: "product_manager",
				avatar: "product-manager",
			},
			{
				id: "noor",
				display_name: "Noor",
				role: "software_developer",
				avatar: "extra-2",
			},
		],
		policy: {
			permissions: { run_commands: true, push: false },
			judgment: { required: "always", questions: [BUDGET], judge: "auto" },
			integration: "auto_merge",
		},
		budgets: {},
	},
};
const FOLDER = "/home/me/.config/farik/templates";

/** The `template.preview` answer: Mira stays, Noor joins, Theo is retired, the rest removed. */
const PREVIEW = {
	team: {
		...TEAM,
		agents: [
			TEAM.agents[0],
			{ ...TEAM.agents[3], status: "retired" },
			agent("noor", "Noor", "software_developer", "extra-2"),
		],
	},
	kept: ["mira"],
	retired: ["theo"],
	removed: ["sol", "ada", "kai"],
	added: ["noor"],
	effects: ["Theo, the Developer, is retired.", "Noor joins as a Developer."],
	errors: [],
	digest: "a".repeat(64),
};

/** The latest `name` query the page asked. */
async function query(s: FakeSocket, name: string) {
	return waitFor(() => {
		const f = s
			.calls("query")
			.filter((q) => q.params.name === name)
			.at(-1);
		if (!f) throw new Error(`no ${name} was asked`);
		return f;
	});
}

describe("team templates", () => {
	it("saves_the_team_as_a_template", async () => {
		const { container, s } = await opened("/team");
		fireEvent.click(
			await screen.findByRole("button", { name: en.templateSaveOpen }),
		);
		const dialog = await screen.findByRole("dialog", { name: en.saveTitle });
		expect(
			within(dialog).getByText("Mira, Sol, Ada, Theo and Kai"),
		).toBeTruthy();
		fireEvent.change(
			within(dialog).getByRole("textbox", { name: en.saveName }),
			{
				target: { value: "My usual team" },
			},
		);
		fireEvent.click(within(dialog).getByRole("button", { name: en.saveSave }));
		const first = await sent(s, "template.save");
		expect(first.params).toEqual({ name: "My usual team" });
		await s.fail(first, -32005, "a template is called My usual team already", {
			errors: [{ path: "/name", message: "taken", code: "template_exists" }],
		});
		expect((await within(dialog).findByRole("alert")).textContent).toBe(
			t("templateExists", { name: "My usual team" }),
		);
		expect(within(dialog).queryByText(/already$/)).toBeNull();
		expect(
			within(dialog).queryByRole("button", { name: en.saveSave }),
		).toBeNull();
		await expectNoAxeViolations(container);

		fireEvent.click(
			within(dialog).getByRole("button", { name: en.saveReplace }),
		);
		await waitFor(() => expect(s.calls("template.save")).toHaveLength(2));
		const second = s.calls("template.save")[1] as never as {
			params: unknown;
		};
		expect(second.params).toEqual({ name: "My usual team", replace: true });
		await s.reply(second as never, { slug: "my-usual-team" });
		const line = await screen.findByRole("status");
		expect(line.textContent).toBe(
			`${en.savedAsBefore}My usual team${en.savedAsAfter}${en.settings}.`,
		);
		expect(screen.queryByRole("dialog")).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("shows_what_changes_before_using", async () => {
		const { container, s } = await opened("/team");
		fireEvent.click(
			await screen.findByRole("button", { name: en.templateUseOpen }),
		);
		await answerQuery(s, "templates.list", {
			folder: FOLDER,
			templates: [PAIR],
			unreadable: [],
		});
		const dialog = await screen.findByRole("dialog", {
			name: en.templateUseOpen,
		});
		const pair = (await within(dialog).findByRole("radio", {
			name: /^Pair/,
		})) as HTMLInputElement;
		expect(pair.checked).toBe(true);
		await expectNoAxeViolations(container);
		fireEvent.click(within(dialog).getByRole("button", { name: en.useShow }));
		const asked = await query(s, "template.preview");
		expect(asked.params.params).toEqual({ slug: "pair" });
		await s.reply(asked, PREVIEW);
		const preview = await screen.findByRole("dialog", {
			name: t("usePreviewTitle", { name: "Pair" }),
		});
		const group = (name: string) =>
			within(preview)
				.getByRole("region", { name: new RegExp(`^${name}`) })
				.querySelectorAll("li strong");
		const names = (name: string) => [...group(name)].map((n) => n.textContent);
		expect(names(en.useStays)).toEqual(["Mira"]);
		expect(names(en.useJoins)).toEqual(["Noor"]);
		expect(names(en.useRetired)).toEqual(["Theo"]);
		expect(names(en.useRemoved)).toEqual(["Sol", "Ada", "Kai"]);
		const changes = within(preview).getByRole("region", {
			name: en.useChanges,
		});
		expect(
			within(changes).getByText("Noor joins as a Developer."),
		).toBeTruthy();
		expect(s.calls("template.apply")).toHaveLength(0);
		await expectNoAxeViolations(container);

		fireEvent.click(within(preview).getByRole("button", { name: en.useApply }));
		const applied = await sent(s, "template.apply");
		expect(applied.params).toEqual({ slug: "pair", digest: PREVIEW.digest });
		const asks = () =>
			s.calls("query").filter((q) => q.params.name === "team.get").length;
		const before = asks();
		await s.reply(applied, PREVIEW);
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		// The Team page reads the team again at once, not on the next event, so "Add someone"
		// is not built from the team as it was.
		await waitFor(() => expect(asks()).toBe(before + 1));
		await s.reply(await query(s, "team.get"), {
			team: PREVIEW.team,
			agents: EFFECTIVE,
			judges: JUDGES,
			max_agents: 7,
		});
		const cards = await screen.findByRole("list", { name: en.teamMembers });
		await waitFor(() =>
			expect(
				within(cards)
					.getAllByRole("listitem")
					.map((li) => li.querySelector("strong")?.textContent),
			).toEqual(["Mira", "Noor"]),
		);
	});

	it("says_when_the_saved_team_changed_since_the_preview", async () => {
		const { container, s } = await opened("/team");
		fireEvent.click(
			await screen.findByRole("button", { name: en.templateUseOpen }),
		);
		await answerQuery(s, "templates.list", {
			folder: FOLDER,
			templates: [PAIR],
			unreadable: [],
		});
		const dialog = await screen.findByRole("dialog", {
			name: en.templateUseOpen,
		});
		await within(dialog).findByRole("radio", { name: /^Pair/ });
		fireEvent.click(within(dialog).getByRole("button", { name: en.useShow }));
		await s.reply(await query(s, "template.preview"), PREVIEW);
		const preview = await screen.findByRole("dialog", {
			name: t("usePreviewTitle", { name: "Pair" }),
		});
		fireEvent.click(within(preview).getByRole("button", { name: en.useApply }));
		await s.fail(await sent(s, "template.apply"), -32005, "changed", {
			errors: [
				{ path: "/digest", message: "changed", code: "template_changed" },
			],
		});
		expect((await within(preview).findByRole("alert")).textContent).toBe(
			en.templateChanged,
		);
		await expectNoAxeViolations(container);

		// As the words say: Back, look again, and use what is shown now.
		fireEvent.click(within(preview).getByRole("button", { name: en.back }));
		const list = await screen.findByRole("dialog", {
			name: en.templateUseOpen,
		});
		const previews = s
			.calls("query")
			.filter((q) => q.params.name === "template.preview").length;
		fireEvent.click(within(list).getByRole("button", { name: en.useShow }));
		await waitFor(() =>
			expect(
				s.calls("query").filter((q) => q.params.name === "template.preview"),
			).toHaveLength(previews + 1),
		);
		const fresh = { ...PREVIEW, digest: "b".repeat(64) };
		await s.reply(await query(s, "template.preview"), fresh);
		const again = await screen.findByRole("dialog", {
			name: t("usePreviewTitle", { name: "Pair" }),
		});
		fireEvent.click(within(again).getByRole("button", { name: en.useApply }));
		await waitFor(() => expect(s.calls("template.apply")).toHaveLength(2));
		const second = s.calls("template.apply")[1] as never as {
			params: unknown;
		};
		expect(second.params).toEqual({ slug: "pair", digest: fresh.digest });
		await s.reply(second as never, fresh);
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("disables_use_when_the_result_is_refused", async () => {
		const paused = {
			...TEAM,
			agents: [
				...TEAM.agents.map((a) =>
					a.id === "theo" ? { ...a, status: "paused" } : a,
				),
				agent("rowan", "Rowan", "software_developer", "extra-2"),
			],
		};
		const { container, s } = await opened("/team", paused);
		fireEvent.click(
			await screen.findByRole("button", { name: en.templateUseOpen }),
		);
		await answerQuery(s, "templates.list", {
			folder: FOLDER,
			templates: [PAIR],
			unreadable: [],
		});
		const dialog = await screen.findByRole("dialog", {
			name: en.templateUseOpen,
		});
		await within(dialog).findByRole("radio", { name: /^Pair/ });
		fireEvent.click(within(dialog).getByRole("button", { name: en.useShow }));
		await s.reply(await query(s, "template.preview"), {
			team: {
				...paused,
				agents: paused.agents.map((a) =>
					["mira", "theo"].includes(a.id) ? a : { ...a, status: "retired" },
				),
			},
			kept: ["mira", "theo"],
			retired: ["sol", "ada", "kai", "rowan"],
			removed: [],
			added: [],
			effects: [],
			errors: [
				{
					path: "/agents",
					message:
						"A team needs an active Software Developer to do the work, and this one has none.",
					code: "needs_developer",
				},
			],
		});
		const preview = await screen.findByRole("dialog", {
			name: t("usePreviewTitle", { name: "Pair" }),
		});
		const alert = within(preview).getByRole("alert");
		expect(alert.textContent).toBe(
			`${en.useCannot}${t("templateNoActive", { role: "Developer" })} ${t(
				"templatePausedRetired",
				{ paused: "Theo", retired: "Rowan" },
			)} ${t("templateResume", { paused: "Theo" })}${en.useNothingChanged}`,
		);
		expect(within(preview).queryByText(/Software Developer/)).toBeNull();
		expect(
			(
				within(preview).getByRole("button", {
					name: en.useApply,
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);
		await expectNoAxeViolations(container);
	});

	it("renames_and_deletes_in_settings", async () => {
		const { container, socket } = await renderApp("/settings");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		const listed = {
			folder: FOLDER,
			templates: [
				{
					...PAIR,
					slug: "my-usual-team",
					template: { ...PAIR.template, name: "My usual team" },
				},
			],
			unreadable: [{ slug: "old-team" }],
		};
		await answerQuery(s, "templates.list", listed);
		const section = await screen.findByRole("region", { name: en.savedTeams });
		expect(
			await within(section).findByText("My usual team", { selector: "strong" }),
		).toBeTruthy();
		expect(
			within(section).getByText("Mira and Noor · saved 28 September 2026"),
		).toBeTruthy();
		expect(
			within(section).getByText(t("savedFolder", { folder: FOLDER })),
		).toBeTruthy();
		// A file Farik cannot read offers Delete alone, with the fixed line.
		const broken = within(section)
			.getByText("old-team", { selector: "code" })
			.closest("li") as HTMLElement;
		expect(within(broken).getByText(en.templateUnreadable)).toBeTruthy();
		expect(
			within(broken)
				.getAllByRole("button")
				.map((b) => b.textContent),
		).toEqual([`${en.savedDelete} old-team`]);
		await expectNoAxeViolations(container);

		fireEvent.click(
			within(section).getByRole("button", { name: "Rename My usual team" }),
		);
		const field = within(section).getByRole("textbox", {
			name: t("savedRenameLabel", { name: "My usual team" }),
		}) as HTMLInputElement;
		expect(field.value).toBe("My usual team");
		fireEvent.change(field, { target: { value: "Shop pair" } });
		await expectNoAxeViolations(container);
		fireEvent.click(
			within(section).getByRole("button", { name: en.savedRenameSave }),
		);
		const renamed = await sent(s, "template.rename");
		expect(renamed.params).toEqual({
			slug: "my-usual-team",
			name: "Shop pair",
		});
		await s.reply(renamed, { slug: "shop-pair" });
		// The list is read again: saving a template records no event.
		await waitFor(() =>
			expect(
				s.calls("query").filter((q) => q.params.name === "templates.list"),
			).toHaveLength(2),
		);
		await answerQuery(s, "templates.list", listed);

		fireEvent.click(
			await within(section).findByRole("button", {
				name: "Delete My usual team",
			}),
		);
		expect(
			within(section).getByText(t("savedDeleteAsk", { name: "My usual team" })),
		).toBeTruthy();
		expect(s.calls("template.delete")).toHaveLength(0);
		await expectNoAxeViolations(container);
		fireEvent.click(
			within(section).getByRole("button", { name: en.savedDeleteNo }),
		);
		expect(
			within(section).queryByRole("button", { name: en.savedDeleteYes }),
		).toBeNull();
		fireEvent.click(
			within(section).getByRole("button", { name: "Delete My usual team" }),
		);
		fireEvent.click(
			within(section).getByRole("button", { name: en.savedDeleteYes }),
		);
		const deleted = await sent(s, "template.delete");
		expect(deleted.params).toEqual({ slug: "my-usual-team" });
	});
});
