import { AVATAR_URLS } from "@catervas/ui";
import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { said } from "../app/refusals.ts";
import { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import {
	ORDERS,
	EFFECTIVE as PROC_EFFECTIVE,
	TEAM as PROC_TEAM,
} from "../test/orders.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
import { NO_MAILBOX } from "../test/sellerMail.ts";
import { onlyInTips } from "../test/tips.ts";

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
	label: "Opus 5.5",
};
/** What the daemon works out for each agent: here, the team said No to commands. */
const EFFECTIVE = [
	["mira", OPUS, ["read", "network"]],
	["sol", OPUS, ["read"]],
	["ada", OPUS, ["read", "write_workspace", "network", "git_local"]],
	["theo", OPUS, ["read", "write_workspace", "git_local"]],
	[
		"kai",
		{ id: "claude-sonnet-5-5", label: "Sonnet 5.5" },
		["read", "network", "write_workspace", "git_local"],
	],
	["iris", OPUS, ["read", "write_workspace", "execute", "git_local"]],
].map(([id, model, tiers]) => ({
	id,
	model: { ...(model as object), effort: "high" },
	tiers,
	base_tiers: tiers,
}));
const MODELS = {
	models: [
		{ id: "claude-opus-5-5", label: "Opus 5.5" },
		{ id: "claude-sonnet-5-5", label: "Sonnet 5.5" },
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
/** What `catervas init` writes, as settings.defaults answers it. */
const DEFAULTS = {
	budgets: {},
	rules: {},
	policy: {
		integration: "auto_merge",
		permissions: { run_commands: true, push: false },
	},
	ui_paths: UI_PATHS,
};

const RULES_PAGES = ["allowed-actions", "spending", "work"];

/** A Settings section, with the queries it asks answered: a rules page asks for the defaults. */
async function settings(
	team: object = TEAM,
	path = "/settings/work",
	answer: object = {},
) {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	const isRules = RULES_PAGES.some((page) => path.endsWith(`/${page}`));
	// Only the rules and the preview ask for the team, on a phone.
	if (isRules || path.endsWith("/your-app"))
		await answerQuery(s, "team.get", {
			team,
			agents: EFFECTIVE,
			max_agents: 7,
			...answer,
		});
	if (isRules) await answerQuery(s, "settings.defaults", DEFAULTS);
	const main = await screen.findByRole("main");
	const part = (name: string) =>
		within(main).getByRole("region", { name }) as HTMLElement;
	return { container, s, main, part };
}

const picked = (el: HTMLElement) => (el as HTMLInputElement).checked;

describe("team page", () => {
	afterEach(() => localStorage.clear());

	it("lists_the_team_with_the_first_day_line", async () => {
		const { container, s } = await opened("/team");
		const list = await screen.findByRole("list", { name: en.teamMembers });
		expect(within(list).getAllByRole("listitem")).toHaveLength(5);
		expect(within(list).getByText("Ada persona")).toBeTruthy();
		// Every agent's model by name and its effort in a word, its own or its role's, never an id.
		expect(within(list).getAllByText("Opus 5.5")).toHaveLength(4);
		expect(within(list).getByText("Sonnet 5.5")).toBeTruthy();
		expect(within(list).getAllByText("High")).toHaveLength(5);
		expect(within(list).queryByText(/claude-/)).toBeNull();
		expect(
			within(list)
				.getByRole("link", { name: "Edit Theo" })
				.getAttribute("href"),
		).toBe("/team/theo");
		expect(screen.getByText(en.firstDay, { exact: false })).toBeTruthy();
		expect(screen.getByText("5 agents work on Corner Bakery.")).toBeTruthy();
		for (const line of en.teamPageRules.split("\n")) onlyInTips(line);
		const changes = screen.getByRole("region", { name: en.teamChangesTitle });
		expect(
			within(changes)
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual(en.teamChangesBody.split("\n"));
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

	it("gives_one_agent_alone_no_team_to_add_to", async () => {
		await opened("/team");
		await screen.findByRole("list", { name: en.teamMembers });
		expect(screen.getByRole("button", { name: en.teamAdd })).toBeTruthy();
		expect(
			screen.getByRole("button", { name: en.templateUseOpen }),
		).toBeTruthy();
		cleanup();

		const alone = {
			...TEAM,
			agents: [TEAM.agents[3] as (typeof TEAM.agents)[number]],
			policy: { ...TEAM.policy, solo: true },
		};
		await opened("/team", alone);
		const list = await screen.findByRole("list", { name: en.teamMembers });
		expect(within(list).getAllByRole("listitem")).toHaveLength(1);
		expect(screen.queryByRole("button", { name: en.teamAdd })).toBeNull();
		expect(
			screen.queryByRole("button", { name: en.templateUseOpen }),
		).toBeNull();
		expect(
			screen.queryByRole("button", { name: en.templateSaveOpen }),
		).toBeNull();
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
			within(cost).getByText("so a day costs more", { exact: false }),
		).toBeTruthy();
	});

	it("names_the_designers_own_model", async () => {
		const { container, socket } = await renderApp("/team");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", {
			team: {
				...TEAM,
				agents: [
					agent("mira", "Mira", "product_manager", "product_manager"),
					agent("theo", "Theo", "software_developer", "software_developer"),
					agent("ivo", "Ivo", "ui_ux_designer", "extra-1"),
				],
			},
			agents: [
				["mira", OPUS],
				["theo", { id: "claude-sonnet-5-5", label: "Sonnet 5.5" }],
				["ivo", OPUS],
			].map(([id, model]) => ({
				id,
				model: { ...(model as object), effort: "high" },
				tiers: ["read"],
				base_tiers: ["read"],
			})),
			max_agents: 7,
		});
		await answerQuery(s, "models.list", MODELS);
		const cost = await screen.findByRole("region", { name: en.teamCostTitle });
		expect(
			within(cost).getByText("Ivo works with Opus 5.5, so a day costs more."),
		).toBeTruthy();
		expect(container.textContent).not.toContain("the same model");
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
					task_id: "CTV-21",
				},
				{
					agent_id: "kai",
					state: "waiting",
					line: "Waiting on you: Kai asks which branch",
					task_id: "CTV-3",
				},
			],
		});
		expect(
			await within(card).findByText("Waiting for Mira to approve a plan"),
		).toBeTruthy();
		expect(within(card).queryByText(en.agentActive)).toBeNull();
		const kai = within(list)
			.getByText("Kai persona")
			.closest("li") as HTMLElement;
		expect(await within(kai).findByText("Waiting on you")).toBeTruthy();
		expect(kai.textContent).not.toContain("which branch");
		expect(face.style.getPropertyValue("--ring")).toBe(
			"var(--catervas-color-role-ui-ux-designer)",
		);
		// The cost section says what the Designer, and its plans, add.
		const cost = screen.getByRole("region", { name: en.teamCostTitle });
		expect(
			within(cost)
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual([
			"Iris works with Opus 5.5, so a day costs more.",
			"Before changing a screen, Iris writes a plan, which costs a little too.",
		]);
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

	it("writes_out_what_hides_text_in_an_agents_activity_line", async () => {
		const { s } = await opened("/team");
		const list = await screen.findByRole("list", { name: en.teamMembers });
		const card = within(list)
			.getByText("Kai persona")
			.closest("li") as HTMLElement;
		// A waiting order's line holds the seller, which the agent wrote: U+202E would reverse the
		// words after it, so the owner reads it written out.
		await answerQuery(s, "team.activity", {
			activity: [
				{
					agent_id: "kai",
					state: "idle",
					line: "Waiting on you: Kai set up an order from Acme\u202e Co: 59.98 USD",
					task_id: "CTV-21",
				},
			],
		});
		expect(
			await within(card).findByText(
				"Waiting on you: Kai set up an order from Acme\\u{202e} Co: 59.98 USD",
			),
		).toBeTruthy();
		expect(card.textContent).not.toContain("\u202e");
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
			"Procurement Specialist",
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

	it("the_team_page_adds_procurement", async () => {
		const { s } = await opened("/team");
		const select = (await screen.findByLabelText(
			en.teamAddRole,
		)) as HTMLSelectElement;
		// The Procurement Specialist is the last role offered, and is not suggested.
		expect(
			within(select)
				.getAllByRole("option")
				.map((option) => option.textContent)
				.at(-1),
		).toBe("Procurement Specialist");
		fireEvent.change(select, { target: { value: "procurement_specialist" } });
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		const team = await saved(s);
		expect(team.agents.at(-1)).toMatchObject({
			display_name: "Noor",
			role: "procurement_specialist",
			avatar: "extra-5",
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

	it("shows_the_agents_picture_beside_its_title", async () => {
		const { container } = await opened("/team/mira");
		const title = await screen.findByRole("heading", {
			level: 1,
			name: "Mira, your Product Manager",
		});
		expect(title.previousElementSibling?.getAttribute("src")).toBe(
			AVATAR_URLS["product-manager"],
		);
		await expectNoAxeViolations(container);
	});

	it("offers_save_only_once_something_changed", async () => {
		await opened("/team/theo");
		const save = (await screen.findByRole("button", {
			name: en.agentSave,
		})) as HTMLButtonElement;
		expect(save.disabled).toBe(true);
		fireEvent.click(screen.getByRole("radio", { name: "Low" }));
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

		fireEvent.click(screen.getByRole("radio", { name: "Low" }));
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
		fireEvent.click(screen.getByRole("radio", { name: "Low" }));
		const checked = await validated(s, ["Kai now works quickly."]);
		expect(one(checked, "kai").model).toEqual({ effort: "low" });
	});

	it("opens_the_agent's_advanced_settings_under_its_button", async () => {
		const { container } = await opened("/team/ada");
		await screen.findByRole("heading", { name: "Ada, your Architect" });
		expect(screen.queryByRole("switch", { name: en.tierNetwork })).toBeNull();
		expect(
			screen.queryByRole("button", { name: en.connectorCustomAdd }),
		).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: en.advancedSwitch }));
		const list = screen.getByRole("list", { name: en.advanced });
		expect(within(list).getAllByRole("switch")).toHaveLength(7);
		expect(
			within(list).getByRole("button", { name: en.connectorCustomAdd }),
		).toBeTruthy();
		expect(screen.getByRole("button", { name: en.advancedHide })).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("starts_closed_on_each_page", async () => {
		// The old global switch's key, left in a browser, opens nothing.
		localStorage.setItem("catervas.advanced", "true");
		await settings(WITH_IRIS, "/settings/work");
		fireEvent.click(
			await screen.findByRole("button", { name: en.advancedSwitch }),
		);
		expect(await screen.findByRole("list", { name: en.advanced })).toBeTruthy();
		cleanup();

		await opened("/team/ada");
		await screen.findByRole("heading", { name: "Ada, your Architect" });
		expect(screen.queryByRole("list", { name: en.advanced })).toBeNull();
		expect(
			screen.getByRole("button", { name: en.advancedSwitch }),
		).toBeTruthy();
	});

	it("toggles_tiers_in_advanced", async () => {
		const { container, s } = await opened("/team/ada");
		await screen.findByRole("heading", { name: "Ada, your Architect" });
		expect(screen.queryByRole("switch", { name: en.tierNetwork })).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: en.advancedSwitch }));
		const tiers = screen.getByRole("list", { name: en.advanced });
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

	it("lifts_a_team_path_for_one_agent_and_adds_its_own", async () => {
		const team = { ...TEAM, rules: { protected_paths: ["infra/**"] } };
		const { s } = await opened("/team/ada", team);
		const rules = within(
			await screen.findByRole("region", {
				name: t("agentRulesTitle", { name: "Ada" }),
			}),
		);
		// The team's own off-limits file is listed, with a way to lift it for Ada alone.
		fireEvent.click(
			rules.getByRole("button", {
				name: `${t("agentLift", { name: "Ada" })} infra/**`,
			}),
		);
		fireEvent.change(rules.getByLabelText(en.filesAdd), {
			target: { value: "notes/**" },
		});
		// The files come first, so their Add is the first.
		fireEvent.click(
			rules.getAllByRole("button", {
				name: en.filesAddButton,
			})[0] as HTMLElement,
		);
		fireEvent.change(rules.getByLabelText(en.sessionCalls), {
			target: { value: "50" },
		});
		await validated(s, []);
		fireEvent.click(screen.getByRole("button", { name: en.agentSave }));
		const sentTeam = await saved(s);
		const ada = one(sentTeam, "ada");
		expect(ada.rules).toEqual({
			lifted_paths: ["infra/**"],
			protected_paths: ["notes/**"],
		});
		expect(ada.session).toEqual({ max_tool_calls: 50 });
		// The team's own rules are unchanged.
		expect(sentTeam.rules).toEqual({ protected_paths: ["infra/**"] });
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
		await s.reply(again, { team: TEAM, agents: EFFECTIVE });
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
		fireEvent.click(await screen.findByRole("radio", { name: "Low" }));
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
			screen.queryByText("Off: Iris can’t see your app, so gets no work."),
		).toBeNull();
		fireEvent.click(hers);
		expect(
			screen.getByText("Off: Iris can’t see your app, so gets no work."),
		).toBeTruthy();
	});

	it("saves_the_preview_in_settings", async () => {
		const { container, s } = await settings(WITH_IRIS, "/settings/your-app");
		const preview = await screen.findByRole("region", {
			name: en.previewTitle,
		});
		expect(preview.id).toBe("preview");
		const lines = (key: keyof typeof en) =>
			t(key, { designer: "Iris" }).split("\n");
		const items = within(preview)
			.getAllByRole("listitem")
			.map((i) => i.textContent);
		for (const line of lines("previewLead")) expect(items).toContain(line);
		for (const line of lines("previewMore")) onlyInTips(line);
		for (const key of [
			"previewPrepareHint",
			"previewStartHint",
			"previewPortHint",
			"previewPathHint",
		] as const)
			for (const line of lines(key)) onlyInTips(line);
		const described = (name: string, key: keyof typeof en) => {
			const box = within(preview).getByRole("textbox", { name });
			const text = (box.getAttribute("aria-describedby") ?? "")
				.split(" ")
				.map((id) => document.getElementById(id)?.textContent ?? "")
				.join(" ");
			expect(text).toContain(lines(key)[0]);
		};
		described(en.previewPrepare, "previewPrepareHint");
		described(en.previewStart, "previewStartHint");
		described(en.previewPort, "previewPortHint");
		described(en.previewPath, "previewPathHint");
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

		// A team without a Designer is told to add one, and never asked.
		await settings(TEAM, "/settings/your-app");
		expect(screen.queryByRole("region", { name: en.previewTitle })).toBeNull();
		expect(await screen.findByText(en.yourAppNone)).toBeTruthy();
	});

	it("says_whether_the_apps_settings_file_is_there", async () => {
		const present = t("previewEnvPresent", { designer: "Iris" });
		const absent = en.previewEnvAbsent;
		const region = async () =>
			await screen.findByRole("region", { name: en.previewTitle });
		await settings(WITH_IRIS, "/settings/your-app", {
			preview_env_present: true,
		});
		let preview = await region();
		expect(within(preview).getByText(present)).toBeTruthy();
		expect(within(preview).queryByText(absent)).toBeNull();
		const tip = document.getElementById("previewEnv-info");
		expect(tip).not.toBeNull();
		for (const line of en.previewEnvMore.split("\n"))
			expect(
				within(preview)
					.getAllByRole("listitem")
					.map((item) => item.textContent),
			).toContain(line);
		cleanup();

		for (const answer of [{ preview_env_present: false }, {}]) {
			await settings(WITH_IRIS, "/settings/your-app", answer);
			preview = await region();
			expect(within(preview).getByText(absent)).toBeTruthy();
			expect(within(preview).queryByText(present)).toBeNull();
			cleanup();
		}
	});

	it("edits_the_ui_paths_in_advanced", async () => {
		// With Advanced off the page does not ask for the patterns.
		await settings(WITH_IRIS, "/settings/work");
		expect(screen.queryByRole("region", { name: en.uiPathsTitle })).toBeNull();
		cleanup();
		const { container, s } = await settings(WITH_IRIS, "/settings/work");
		fireEvent.click(
			await screen.findByRole("button", { name: en.advancedSwitch }),
		);
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
		const { container, s, part } = await settings(
			TEAM,
			"/settings/allowed-actions",
		);
		for (const line of en.rulesNextSession.split("\n")) onlyInTips(line);
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
			within(may).getByRole("radio", { name: /^Yes, to its own branches/ }),
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

		// Put back the default: the answers `catervas init` writes.
		fireEvent.click(within(may).getByRole("button", { name: en.putBack }));
		expect(
			picked(
				within(may).getByRole("radio", {
					name: /^Yes, the Developer and Architect may/,
				}),
			),
		).toBe(true);
		expect(
			picked(
				within(may).getByRole("radio", { name: /^No, keep work in progress/ }),
			),
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

		// The same daily limit as the Costs page's, on its own page.
		cleanup();
		const next = await settings(TEAM, "/settings/spending");
		const { s: s2, part: part2 } = next;
		const spend = part2(en.rulesSpendTitle);
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
		const limited = await validated(s2, [
			"The team may spend up to $15 a day.",
		]);
		expect(limited.budgets).toEqual({ daily_usd: 15 });
		expect(
			await within(spend).findByText("The team may spend up to $15 a day."),
		).toBeTruthy();
		fireEvent.click(within(spend).getByRole("button", { name: en.agentSave }));
		await waitFor(() => expect(s2.calls("team.save")).toHaveLength(1));
		expect((await saved(s2)).budgets).toEqual({ daily_usd: 15 });
	});

	it("says_finished_work_opens_a_pull_request_from_settings", async () => {
		const { part } = await settings(TEAM, "/settings/work", { online: true });

		// Finished work opens a pull request when the project is online (ADR 0061): said, and
		// nothing to choose.
		const finish = part(en.rulesFinish);
		expect(within(finish).getByText(en.finishLead)).toBeTruthy();
		expect(within(finish).queryByRole("radio")).toBeNull();
		cleanup();

		// Not online: it waits on this computer for a merge by hand.
		const local = await settings(TEAM, "/settings/work", { online: false });
		expect(
			within(local.part(en.rulesFinish)).getByText(en.finishLeadLocal),
		).toBeTruthy();
	});

	it("sets_the_integration_branch", async () => {
		const { s, part } = await settings(TEAM, "/settings/work");
		const branch = part(en.rulesBranch);
		const field = within(branch).getByLabelText(en.rulesBranchLabel);
		fireEvent.change(field, { target: { value: "catervas/next" } });
		await validated(s, ["Finished work goes into catervas/next."]);
		const save = () =>
			within(branch).getByRole("button", {
				name: en.agentSave,
			}) as HTMLButtonElement;
		await waitFor(() => expect(save().disabled).toBe(false));
		fireEvent.click(save());
		expect((await saved(s)).policy).toMatchObject({
			integration_branch: "catervas/next",
		});
		cleanup();

		// Emptied, the key is left out and finished work goes to the main branch again.
		const named = {
			...TEAM,
			policy: { ...TEAM.policy, integration_branch: "catervas/next" },
		};
		const again = await settings(named, "/settings/work");
		const second = again.part(en.rulesBranch);
		const input = within(second).getByLabelText(
			en.rulesBranchLabel,
		) as HTMLInputElement;
		expect(input.value).toBe("catervas/next");
		fireEvent.change(input, { target: { value: "" } });
		await validated(again.s, []);
		const clear = within(second).getByRole("button", {
			name: en.agentSave,
		}) as HTMLButtonElement;
		await waitFor(() => expect(clear.disabled).toBe(false));
		fireEvent.click(clear);
		expect("integration_branch" in (await saved(again.s)).policy).toBe(false);
		cleanup();

		// A project of several repositories keeps each one's branch in its own file.
		const members = await settings(TEAM, "/settings/work", {
			members_present: true,
		});
		expect(
			within(members.main).queryByRole("region", { name: en.rulesBranch }),
		).toBeNull();
	});

	it("switches_the_document_format", async () => {
		// TEAM has no document_format: a team without the key writes HTML.
		expect("document_format" in TEAM.policy).toBe(false);
		const { s, part } = await settings(TEAM, "/settings/work");
		const documents = part(en.rulesDocuments);
		const toggle = within(documents).getByRole("switch", {
			name: en.documentsHtml,
		});
		expect(toggle.getAttribute("aria-checked")).toBe("true");
		for (const line of en.documentsHtmlNote.split("\n")) onlyInTips(line);
		fireEvent.click(toggle);
		expect(toggle.getAttribute("aria-checked")).toBe("false");
		await validated(s, []);
		const save = () =>
			within(documents).getByRole("button", {
				name: en.agentSave,
			}) as HTMLButtonElement;
		await waitFor(() => expect(save().disabled).toBe(false));
		fireEvent.click(save());
		expect((await saved(s)).policy).toMatchObject({
			document_format: "markdown",
		});
	});

	it("keeps_documents_on_this_computer", async () => {
		const { s, part } = await settings(TEAM, "/settings/work", {
			documents_mode: "committed",
		});
		const documents = part(en.rulesDocuments);
		const local = within(documents).getByRole("switch", {
			name: en.documentsLocal,
		});
		expect(local.getAttribute("aria-checked")).toBe("false");
		for (const line of en.documentsLocalNote.split("\n")) onlyInTips(line);

		// Turning it on asks first, and "Cancel" changes nothing.
		fireEvent.click(local);
		let dialog = await screen.findByRole("dialog", { name: en.keepHereTitle });
		for (const line of en.keepHereLines.split("\n"))
			expect(within(dialog).getByText(line)).toBeTruthy();
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.keepHereCancel }),
		);
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("documents.set_local")).toEqual([]);

		// "Keep them here" sends the choice; a refusal is said in words.
		fireEvent.click(local);
		dialog = await screen.findByRole("dialog", { name: en.keepHereTitle });
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.keepHereYes }),
		);
		const frame = await sent(s, "documents.set_local");
		expect(frame.params).toEqual({ local: true });
		await s.fail(frame as never, -32005, "refused", {
			errors: [
				{
					path: "/local",
					code: "documents_busy",
					message: "Some documents have changes",
				},
			],
		});
		expect(await screen.findByText(en.refuseDocumentsBusy)).toBeTruthy();
		cleanup();

		// While the pull request waits, the switch is off limits and says why.
		const moving = {
			...TEAM,
			policy: { ...TEAM.policy, documents_local: true },
		};
		const taking = await settings(moving, "/settings/work", {
			documents_mode: "taking_out",
		});
		const documentsTaking = taking.part(en.rulesDocuments);
		const disabled = within(documentsTaking).getByRole("switch", {
			name: en.documentsLocal,
		});
		expect(disabled.getAttribute("aria-checked")).toBe("true");
		expect((disabled as HTMLInputElement).disabled).toBe(true);
		expect(
			within(documentsTaking).getByText(en.documentsTakingOut),
		).toBeTruthy();
		cleanup();

		// Turned on and out of the repository, it can be turned off again, with its own question.
		const on = await settings(moving, "/settings/work", {
			documents_mode: "local",
		});
		fireEvent.click(
			within(on.part(en.rulesDocuments)).getByRole("switch", {
				name: en.documentsLocal,
			}),
		);
		const back = await screen.findByRole("dialog", { name: en.putBackTitle });
		fireEvent.click(within(back).getByRole("button", { name: en.putBackYes }));
		expect((await sent(on.s, "documents.set_local")).params).toEqual({
			local: false,
		});
	});

	it("puts_back_the_default_document_format", async () => {
		const markdown = {
			...TEAM,
			policy: { ...TEAM.policy, document_format: "markdown" },
		};
		const { s, part } = await settings(markdown, "/settings/work");
		const documents = part(en.rulesDocuments);
		const toggle = within(documents).getByRole("switch", {
			name: en.documentsHtml,
		});
		expect(toggle.getAttribute("aria-checked")).toBe("false");
		fireEvent.click(
			within(documents).getByRole("button", { name: en.putBack }),
		);
		expect(toggle.getAttribute("aria-checked")).toBe("true");
		await validated(s, []);
		const save = within(documents).getByRole("button", {
			name: en.agentSave,
		}) as HTMLButtonElement;
		await waitFor(() => expect(save.disabled).toBe(false));
		fireEvent.click(save);
		expect((await saved(s)).policy).toMatchObject({ document_format: "html" });
	});

	describe("the rewrite", () => {
		const doc = (path: string) => ({ path, name: path, twin: null });
		const treeOf = (...paths: string[]) => ({
			integration: "auto_merge",
			folders: [
				{
					folder: "docs/catervas/architecture",
					name: "Architecture",
					role: "architect",
					agent_id: "ada",
					documents: paths.map(doc),
				},
			],
		});
		const HTML = {
			...TEAM,
			policy: { ...TEAM.policy, document_format: "html" },
		};
		const MARKDOWN = {
			...TEAM,
			policy: { ...TEAM.policy, document_format: "markdown" },
		};
		const rewrite = (name: string) =>
			screen.queryByRole("button", { name }) as HTMLElement | null;

		it("offers_the_rewrite_while_markdown_remains", async () => {
			const { s, part } = await settings(HTML, "/settings/work");
			await answerQuery(
				s,
				"files.tree",
				treeOf(
					"docs/catervas/architecture/overview.md",
					"docs/catervas/marketing/plans/MP-1.md",
				),
			);
			const documents = part(en.rulesDocuments);
			const button = await within(documents).findByRole("button", {
				name: en.documentsRewrite,
			});
			for (const line of en.documentsRewriteNote.split("\n")) onlyInTips(line);
			fireEvent.click(button);
			const frame = await sent(s, "files.rewrite");
			expect(frame.params).toEqual({});
			await s.reply(frame, { tasks: ["t1"] });
			expect(
				await within(documents).findByText(en.documentsRewriteStarted),
			).toBeTruthy();
		});

		it("hides_the_rewrite_without_markdown_to_rewrite", async () => {
			// Only .html documents and a marketing plan.
			const only = treeOf(
				"docs/catervas/architecture/overview.html",
				"docs/catervas/marketing/plans/MP-1.md",
				"docs/catervas/marketing/plans/MP-1.agent.md",
			);
			const { s, part } = await settings(HTML, "/settings/work");
			await answerQuery(s, "files.tree", only);
			await within(part(en.rulesDocuments)).findByRole("switch", {
				name: en.documentsHtml,
			});
			await act(async () => {});
			expect(rewrite(en.documentsRewrite)).toBeNull();
			cleanup();

			// A Markdown team has none either, whatever its tree holds.
			const md = await settings(MARKDOWN, "/settings/work");
			await answerQuery(
				md.s,
				"files.tree",
				treeOf("docs/catervas/architecture/overview.md"),
			);
			await within(md.part(en.rulesDocuments)).findByRole("switch", {
				name: en.documentsHtml,
			});
			await act(async () => {});
			expect(rewrite(en.documentsRewrite)).toBeNull();
		});

		it("offers_the_rewrite_for_a_twin_or_a_note_beside_the_plans", async () => {
			for (const tree of [
				{
					...treeOf(),
					folders: [
						{
							...treeOf().folders[0],
							documents: [
								{
									path: "docs/catervas/architecture/spec.html",
									name: "spec",
									twin: "docs/catervas/architecture/spec.agent.md",
								},
							],
						},
					],
				},
				treeOf("docs/catervas/marketing/plans/MP-1-notes.md"),
			]) {
				const { s, part } = await settings(HTML, "/settings/work");
				await answerQuery(s, "files.tree", tree);
				expect(
					await within(part(en.rulesDocuments)).findByRole("button", {
						name: en.documentsRewrite,
					}),
				).toBeTruthy();
				cleanup();
			}
		});

		it("sends_one_rewrite_while_one_is_pending", async () => {
			const { s, part } = await settings(HTML, "/settings/work");
			await answerQuery(
				s,
				"files.tree",
				treeOf("docs/catervas/architecture/overview.md"),
			);
			const documents = part(en.rulesDocuments);
			const button = await within(documents).findByRole("button", {
				name: en.documentsRewrite,
			});
			fireEvent.click(button);
			const frame = await sent(s, "files.rewrite");
			fireEvent.click(button);
			expect(s.calls("files.rewrite")).toHaveLength(1);
			expect((button as HTMLButtonElement).disabled).toBe(true);
			await s.reply(frame, { tasks: [] });
			expect(
				await within(documents).findByText(en.documentsRewriteStarted),
			).toBeTruthy();
		});

		it("says_why_a_rewrite_was_refused", async () => {
			const { s, part } = await settings(HTML, "/settings/work");
			await answerQuery(
				s,
				"files.tree",
				treeOf("docs/catervas/architecture/overview.md"),
			);
			const documents = part(en.rulesDocuments);
			const refusals = [
				["not_html", en.refuseNotHtml],
				["rewrite_open", en.refuseRewriteOpen],
				["nothing_to_rewrite", en.refuseNothingToRewrite],
			] as const;
			for (const [index, [code, words]] of refusals.entries()) {
				fireEvent.click(
					await within(documents).findByRole("button", {
						name: en.documentsRewrite,
					}),
				);
				await s.fail(
					await waitFor(() => {
						const frame = s.calls("files.rewrite")[index];
						if (!frame) throw new Error("no files.rewrite was sent");
						return frame;
					}),
					-32005,
					"refused",
					{ errors: [{ path: "", message: "no", code }] },
				);
				expect(await within(documents).findByText(words)).toBeTruthy();
				expect(
					within(documents).queryByText(en.documentsRewriteStarted),
				).toBeNull();
			}
		});
	});

	it("keeps_each_rules_draft_until_it_is_saved_or_cancelled", async () => {
		const { s, part } = await settings(TEAM, "/settings/work");
		// The saved team is checked once on load, not once for each part.
		await act(async () => {});
		expect(
			s.calls("query").filter((q) => q.params.name === "team.validate"),
		).toHaveLength(1);

		// A draft survives until it is saved or cancelled.
		const documents = () => part(en.rulesDocuments);
		const html = () =>
			within(documents()).getByRole("switch", { name: en.documentsHtml });
		expect(html().getAttribute("aria-checked")).toBe("true");
		fireEvent.click(html());
		await validated(s, ["New documents are written in Markdown."]);
		// Cancel drops the draft: the part shows the team's own answer again.
		fireEvent.click(
			within(documents()).getByRole("button", { name: en.agentCancel }),
		);
		await waitFor(() =>
			expect(html().getAttribute("aria-checked")).toBe("true"),
		);
		fireEvent.click(html());
		await validated(s, ["New documents are written in Markdown."]);
		fireEvent.click(
			within(documents()).getByRole("button", { name: en.agentSave }),
		);
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
			team: {
				...TEAM,
				policy: { ...TEAM.policy, document_format: "markdown" },
			},
			agents: EFFECTIVE,
			max_agents: 7,
		});
		await waitFor(() =>
			expect(html().getAttribute("aria-checked")).toBe("false"),
		);
	});

	it("shows_a_saved_limit_once_the_team_is_read_again", async () => {
		const { s, part } = await settings(TEAM, "/settings/spending");
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
		await waitFor(() => expect(s.calls("team.save")).toHaveLength(1));
		await s.reply(s.calls("team.save")[0] as never, {});
		const again = await waitFor(() => {
			const all = s.calls("query").filter((q) => q.params.name === "team.get");
			if (all.length === reads) throw new Error("the team was not read again");
			return all.at(-1) as never;
		});
		await s.reply(again, {
			team: { ...TEAM, budgets: { daily_usd: 15 } },
			agents: EFFECTIVE,
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

	it("checks_and_signs_out_of_the_ai_account", async () => {
		const { container, socket } = await renderApp("/settings/account");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "account.status", {
			signed_in: true,
			kind: "subscription",
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		expect(
			await within(row).findByText(
				"Claude, through Claude Code: signed in with your Claude subscription.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		// Check connection runs the check and says its outcome.
		fireEvent.click(within(row).getByRole("button", { name: en.accountCheck }));
		await s.reply(await sent(s, "account.check"), { outcome: "working" });
		expect(await within(row).findByText(en.connectionWorks)).toBeTruthy();
		fireEvent.click(within(row).getByRole("button", { name: en.accountCheck }));
		await waitFor(() => expect(s.calls("account.check")).toHaveLength(2));
		await s.reply(s.calls("account.check")[1] as never, {
			outcome: "failed",
			detail: "Overloaded",
		});
		expect(
			await within(row).findByText("It didn’t work: Overloaded"),
		).toBeTruthy();

		// Sign out is asked twice, then runs Claude Code's logout and shows the way back in.
		fireEvent.click(
			within(row).getByRole("button", { name: en.accountSignOut }),
		);
		expect(within(row).getByText(en.accountConfirm)).toBeTruthy();
		expect(s.calls("account.sign_out")).toHaveLength(0);
		fireEvent.click(
			within(row).getByRole("button", { name: en.accountSignOutYes }),
		);
		const out = await sent(s, "account.sign_out");
		await s.reply(out, { paused: true });
		// The row reads the account again.
		await waitFor(() => {
			const asked = s
				.calls("query")
				.filter((q) => q.params.name === "account.status");
			if (asked.length < 2) throw new Error("the account was not asked again");
		});
		await answerQuery(s, "account.status", { signed_in: false, kind: null });
		expect(await within(row).findByText(en.accountNone)).toBeTruthy();
		expect(
			within(row).getByRole("button", { name: en.signInClaude }),
		).toBeTruthy();
		expect(
			within(row).getByRole("button", { name: en.signInConsole }),
		).toBeTruthy();
		expect(within(row).getByText(en.accountSignedOut)).toBeTruthy();
	});

	it("says_a_failed_sign_out_in_plain_words", async () => {
		const { socket } = await renderApp("/settings/account");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "account.status", {
			signed_in: true,
			kind: "console",
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		fireEvent.click(
			await within(row).findByRole("button", { name: en.accountSignOut }),
		);
		fireEvent.click(
			within(row).getByRole("button", { name: en.accountSignOutYes }),
		);
		await s.fail(
			await sent(s, "account.sign_out"),
			-32005,
			"Claude Code did not sign out: keyring: platform secure storage failure",
		);
		expect(await within(row).findByText(en.signOutFailed)).toBeTruthy();
		expect(within(row).queryByText(/keyring/)).toBeNull();
	});

	it("says_the_account_comes_from_the_environment", async () => {
		const { socket } = await renderApp("/settings/account");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "account.status", {
			signed_in: true,
			kind: "environment",
			environment_variable: "ANTHROPIC_API_KEY",
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		expect(
			await within(row).findByText(
				"It comes from ANTHROPIC_API_KEY where you start Catervas. Remove it there to sign in here instead.",
			),
		).toBeTruthy();
		expect(
			within(row).queryByRole("button", { name: en.accountSignOut }),
		).toBeNull();
		// Check connection stays.
		expect(
			within(row).getByRole("button", { name: en.accountCheck }),
		).toBeTruthy();
	});

	it("offers_sign_in_again_after_a_refused_key", async () => {
		const { socket } = await renderApp("/settings/account");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "account.status", {
			signed_in: true,
			kind: "console",
			key_refused: true,
			kept_key_removed: true,
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		expect(await within(row).findByText(en.connectionKeyRefused)).toBeTruthy();
		expect(
			within(row).getByRole("heading", { name: en.accountSignInAgain }),
		).toBeTruthy();
		expect(
			within(row).getByRole("button", { name: en.signInClaude }),
		).toBeTruthy();
		expect(
			within(row).getByRole("button", { name: en.signInConsole }),
		).toBeTruthy();
		// The key an earlier version kept was removed, and the row says so.
		expect(within(row).getByText(en.accountKeptKeyRemoved)).toBeTruthy();

		// A sign-in that works says so and reads the account again.
		vi.stubGlobal(
			"open",
			vi.fn(() => ({}) as Window),
		);
		fireEvent.click(within(row).getByRole("button", { name: en.signInClaude }));
		await s.reply(await sent(s, "account.sign_in"), {
			url: "https://claude.com/x",
		});
		fireEvent.change(await within(row).findByLabelText(en.signInCode), {
			target: { value: "abc" },
		});
		fireEvent.click(within(row).getByRole("button", { name: en.signInFinish }));
		await s.reply(await sent(s, "account.sign_in_code"), {
			outcome: "working",
		});
		expect(await within(row).findByText(en.accountSignedBack)).toBeTruthy();
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
			integration: "auto_merge",
		},
		budgets: {},
	},
};
const FOLDER = "/home/me/.config/catervas/templates";

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

const builtinTeam = (slug: string, name: string, agents: string[][]) => ({
	slug,
	template: {
		...PAIR.template,
		name,
		agents: agents.map(([id, display_name, role, avatar]) => ({
			id,
			display_name,
			role,
			avatar,
		})),
	},
});
/** `templates.list` with the three ready-made teams and Pair saved. */
const LISTED = {
	folder: FOLDER,
	builtin: [
		builtinTeam("software-product", "Software product", [
			["mira", "Mira", "product_manager", "product-manager"],
			["ada", "Ada", "architect", "architect"],
			["theo", "Theo", "software_developer", "developer"],
			["kai", "Kai", "marketing_specialist", "marketing-specialist"],
		]),
		builtinTeam("online-store", "Online store", [
			["mira", "Mira", "product_manager", "product-manager"],
			["noor", "Noor", "procurement_specialist", "extra-5"],
		]),
		builtinTeam("financial-analysis", "Financial analysis", [
			["mira", "Mira", "product_manager", "product-manager"],
			["noor", "Noor", "finance_specialist", "finance-specialist"],
		]),
	],
	templates: [PAIR],
	unreadable: [],
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
		expect(
			within(line)
				.getByRole("link", { name: en.settings })
				.getAttribute("href"),
		).toBe("/settings/saved-teams");
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
		expect(
			within(dialog)
				.getByRole("link", { name: en.settings })
				.getAttribute("href"),
		).toBe("/settings/saved-teams");
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

	/** Two Product Managers, Mira paused and Pat active; Pair holds Mira and would retire Pat. */
	it("disables_use_when_the_result_is_refused", async () => {
		const paused = {
			...TEAM,
			agents: [
				...TEAM.agents.map((a) =>
					a.id === "mira" ? { ...a, status: "paused" } : a,
				),
				agent("pat", "Pat", "product_manager", "extra-2"),
			],
		};
		const { container, s } = await opened("/team", paused);
		fireEvent.click(
			await screen.findByRole("button", { name: en.templateUseOpen }),
		);
		await answerQuery(s, "templates.list", {
			folder: FOLDER,
			builtin: [],
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
			retired: ["sol", "ada", "kai", "pat"],
			removed: [],
			added: [],
			effects: [],
			errors: [
				{
					path: "/agents",
					message:
						"A team needs an active Product Manager to plan its work, and this one has none.",
					code: "needs_product_manager",
				},
			],
		});
		const preview = await screen.findByRole("dialog", {
			name: t("usePreviewTitle", { name: "Pair" }),
		});
		const alert = within(preview).getByRole("alert");
		expect(alert.textContent).toBe(
			`${en.useCannot}${t("templateNoActive", { role: "Product Manager" })} ${t(
				"templatePausedRetired",
				{ paused: "Mira", retired: "Pat" },
			)} ${t("templateResume", { paused: "Mira" })}${en.useNothingChanged}`,
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
		const { container, socket } = await renderApp("/settings/saved-teams");
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
		// A file Catervas cannot read offers Delete alone, with the fixed line.
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

	it("words_the_builtin_refusal_and_forgets_the_developers", async () => {
		expect(said("template_builtin", { name: "Online store" })).toBe(
			"Catervas comes with a team called Online store. Choose another name.",
		);
		// The Developer is no longer required, so its refusal has no words of its own.
		expect(said("needs_developer")).toBe(en.refuseOther);

		const { socket } = await renderApp("/settings/saved-teams");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "templates.list", {
			folder: FOLDER,
			builtin: [],
			templates: [PAIR],
			unreadable: [],
		});
		const section = await screen.findByRole("region", { name: en.savedTeams });
		fireEvent.click(
			await within(section).findByRole("button", { name: "Rename Pair" }),
		);
		fireEvent.change(
			within(section).getByRole("textbox", {
				name: t("savedRenameLabel", { name: "Pair" }),
			}),
			{ target: { value: "  Online store " } },
		);
		fireEvent.click(
			within(section).getByRole("button", { name: en.savedRenameSave }),
		);
		await s.fail(await sent(s, "template.rename"), -32005, "refused", {
			errors: [{ path: "/name", message: "builtin", code: "template_builtin" }],
		});
		expect(
			await within(section).findByText(
				"Catervas comes with a team called Online store. Choose another name.",
			),
		).toBeTruthy();
	});

	it("settings_lists_saved_teams_only", async () => {
		const { socket } = await renderApp("/settings/saved-teams");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "templates.list", {
			folder: null,
			builtin: [
				{
					slug: "online-store",
					template: { ...PAIR.template, name: "Online store" },
				},
			],
			templates: [],
			unreadable: [],
		});
		const section = await screen.findByRole("region", { name: en.savedTeams });
		expect(await within(section).findByText(en.templateNoFolder)).toBeTruthy();
		expect(within(section).queryByText("Online store")).toBeNull();
	});

	it("lists_the_ready_made_above_the_saved", async () => {
		const { container, s } = await opened("/team");
		fireEvent.click(
			await screen.findByRole("button", { name: en.templateUseOpen }),
		);
		await answerQuery(s, "templates.list", LISTED);
		const dialog = await screen.findByRole("dialog", {
			name: en.templateUseOpen,
		});
		const ready = await within(dialog).findByRole("group", {
			name: en.useBuiltinList,
		});
		const mine = within(dialog).getByRole("group", { name: en.useList });
		expect(
			within(ready)
				.getAllByRole("radio")
				.map((r) => (r as HTMLInputElement).value),
		).toEqual(["software-product", "online-store", "financial-analysis"]);
		expect(
			within(mine)
				.getAllByRole("radio")
				.map((r) => (r as HTMLInputElement).value),
		).toEqual(["pair"]);
		// Ready-made comes first on the page, and the first of them is picked.
		expect(
			ready.compareDocumentPosition(mine) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		const software = within(ready).getByRole("radio", {
			name: /^Software product/,
		}) as HTMLInputElement;
		expect(software.checked).toBe(true);
		expect(
			within(ready).getByText(
				t("builtinMeta", { names: "Mira, Ada, Theo and Kai" }),
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
		// One selection across both groups.
		const pair = within(mine).getByRole("radio", {
			name: /^Pair/,
		}) as HTMLInputElement;
		fireEvent.click(pair);
		expect(pair.checked).toBe(true);
		expect(software.checked).toBe(false);
	});

	it("previews_and_uses_a_ready_made_team", async () => {
		const { s } = await opened("/team");
		fireEvent.click(
			await screen.findByRole("button", { name: en.templateUseOpen }),
		);
		await answerQuery(s, "templates.list", LISTED);
		const dialog = await screen.findByRole("dialog", {
			name: en.templateUseOpen,
		});
		fireEvent.click(
			await within(dialog).findByRole("radio", { name: /^Financial analysis/ }),
		);
		fireEvent.click(within(dialog).getByRole("button", { name: en.useShow }));
		const asked = await query(s, "template.preview");
		expect(asked.params.params).toEqual({ slug: "financial-analysis" });
		await s.reply(asked, PREVIEW);
		const preview = await screen.findByRole("dialog", {
			name: t("usePreviewTitle", { name: "Financial analysis" }),
		});
		fireEvent.click(within(preview).getByRole("button", { name: en.useApply }));
		const applied = await sent(s, "template.apply");
		expect(applied.params).toEqual({
			slug: "financial-analysis",
			digest: PREVIEW.digest,
		});
	});

	it("opens_with_no_saved_team_and_no_folder", async () => {
		const { s } = await opened("/team");
		fireEvent.click(
			await screen.findByRole("button", { name: en.templateUseOpen }),
		);
		await answerQuery(s, "templates.list", {
			...LISTED,
			folder: null,
			templates: [],
		});
		const dialog = await screen.findByRole("dialog", {
			name: en.templateUseOpen,
		});
		const ready = await within(dialog).findByRole("group", {
			name: en.useBuiltinList,
		});
		expect(within(ready).getAllByRole("radio")).toHaveLength(3);
		expect(
			within(dialog).getByRole("heading", { name: en.useList }),
		).toBeTruthy();
		expect(within(dialog).getByText(en.templateNoFolder)).toBeTruthy();
	});

	it("opens_with_no_saved_team_in_a_folder", async () => {
		const { s } = await opened("/team");
		fireEvent.click(
			await screen.findByRole("button", { name: en.templateUseOpen }),
		);
		await answerQuery(s, "templates.list", { ...LISTED, templates: [] });
		const dialog = await screen.findByRole("dialog", {
			name: en.templateUseOpen,
		});
		await within(dialog).findByRole("group", { name: en.useBuiltinList });
		expect(
			within(dialog).getByRole("heading", { name: en.useList }),
		).toBeTruthy();
		expect(within(dialog).getByText(en.startSavedNone)).toBeTruthy();
	});

	it("says_the_team_needs_a_product_manager_alone", async () => {
		await opened("/team");
		const lead = await screen.findByText(/agents work on/);
		expect(lead.textContent).toBe("5 agents work on Corner Bakery.");
		onlyInTips("A team needs a Product Manager.");
	});

	it("refusedLine_knows_no_developer_rule", async () => {
		const team = {
			...TEAM,
			agents: TEAM.agents.map((a) =>
				a.id === "mira" ? { ...a, status: "paused" } : a,
			),
		};
		const refuse = async (code: string) => {
			const { s } = await opened("/team", team);
			fireEvent.click(
				await screen.findByRole("button", { name: en.templateUseOpen }),
			);
			await answerQuery(s, "templates.list", LISTED);
			const dialog = await screen.findByRole("dialog", {
				name: en.templateUseOpen,
			});
			fireEvent.click(
				await within(dialog).findByRole("button", { name: en.useShow }),
			);
			await s.reply(await query(s, "template.preview"), {
				...PREVIEW,
				team,
				kept: ["mira"],
				retired: [],
				errors: [{ path: "/agents", message: "refused", code }],
			});
			const preview = await screen.findByRole("dialog", {
				name: t("usePreviewTitle", { name: "Software product" }),
			});
			return within(preview).getByRole("alert").textContent;
		};
		expect(await refuse("needs_product_manager")).toBe(
			`${en.useCannot}${t("templateNoActive", { role: "Product Manager" })} ${t(
				"templatePaused",
				{ paused: "Mira" },
			)} ${t("templateResume", { paused: "Mira" })}${en.useNothingChanged}`,
		);
		cleanup();
		// The Developer rule is gone: its code is any other refusal.
		expect(await refuse("needs_developer")).toBe(
			`${en.useCannot}${said("needs_developer")} ${en.templateChooseAnother}${en.useNothingChanged}`,
		);
	});
});

describe("info buttons on the agent page", () => {
	afterEach(() => {
		vi.useRealTimers();
		cleanup();
	});

	it("puts every note on the agent page behind an info button", async () => {
		await opened("/team/theo");
		await screen.findByRole("heading", { name: "Theo, your Developer" });
		const say = (key: keyof typeof en) =>
			t(key, { name: "Theo", role: "Developer" });
		for (const key of [
			"agentNextWork",
			"agentPauseNote",
			"agentReplaceNote",
			"agentRetireNote",
			"skillsLead",
		] as const)
			onlyInTips(say(key));
		fireEvent.click(screen.getByRole("button", { name: en.advancedSwitch }));
		for (const key of [
			"tierReadNote",
			"tierWriteNote",
			"tierExecuteNote",
			"tierGitLocalNote",
			"tierNetworkNote",
			"tierGitRemoteNote",
			"tierExternalNote",
			"connectorCustomNote",
		] as const)
			onlyInTips(say(key));
	});

	it("shows_the_model_effort_and_temperature_as_choices_on_the_editor", async () => {
		const { container, s } = await opened("/team/theo");
		await screen.findByRole("heading", { name: "Theo, your Developer" });
		// The model by name and the five efforts, each a segmented group with one note behind info.
		const model = screen.getByRole("group", { name: en.modelLegend });
		expect(
			within(model)
				.getAllByRole("radio")
				.map((r) => (r as HTMLInputElement).labels?.[0]?.textContent),
		).toEqual(["Opus 5.5", "Sonnet 5.5"]);
		expect(
			(
				within(model).getByRole("radio", {
					name: "Opus 5.5",
				}) as HTMLInputElement
			).checked,
		).toBe(true);
		const effort = screen.getByRole("group", { name: en.effortLegend });
		const efforts = ["Low", "Medium", "High", "Extra high", "Max"];
		expect(
			within(effort)
				.getAllByRole("radio")
				.map((r) => (r as HTMLInputElement).labels?.[0]?.textContent),
		).toEqual(efforts);
		expect(
			(within(effort).getByRole("radio", { name: "High" }) as HTMLInputElement)
				.checked,
		).toBe(true);
		for (const line of en.effortInfo.split("\n")) onlyInTips(line);
		// Temperature is a fact, not a control: the model sets it.
		expect(screen.getByText(en.temperatureLegend)).toBeTruthy();
		expect(screen.getByText(en.temperatureValue)).toBeTruthy();
		await expectNoAxeViolations(container);

		// Choosing the model and the top effort goes into the team as the ids the daemon takes.
		fireEvent.click(within(model).getByRole("radio", { name: "Sonnet 5.5" }));
		fireEvent.click(within(effort).getByRole("radio", { name: "Max" }));
		const checked = await validated(s, []);
		expect(one(checked, "theo").model).toEqual({
			id: "claude-sonnet-5-5",
			effort: "max",
		});
	});

	it("puts the Procurement Specialist's notes behind info", async () => {
		const { socket } = await renderApp("/team/ivo");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", {
			team: PROC_TEAM,
			agents: PROC_EFFECTIVE,
			max_agents: 7,
		});
		await answerQuery(s, "models.list", MODELS);
		await answerQuery(s, "skills.list", { skills: [] });
		await answerQuery(s, "purchase_orders.list", ORDERS);
		await answerQuery(s, "procurement_mailbox.get", NO_MAILBOX);
		await screen.findByRole("heading", {
			name: "Ivo, your Procurement Specialist",
		});
		for (const key of ["ordersLead", "mailboxNoneNote"] as const)
			onlyInTips(t(key, { name: "Ivo" }));
	});
});

describe("info buttons on Settings", () => {
	afterEach(() => {
		cleanup();
		localStorage.clear();
	});

	it("keeps Settings to short lines, the rest behind info buttons", async () => {
		const general = await settings(TEAM, "/settings/general");
		await expectNoAxeViolations(general.container);
		cleanup();

		await settings(TEAM, "/settings/allowed-actions");
		for (const line of en.rulesNextSession.split("\n")) onlyInTips(line);
		cleanup();

		const { s } = await settings(TEAM, "/settings/saved-teams");
		await answerQuery(s, "templates.list", {
			folder: FOLDER,
			templates: [
				{
					...PAIR,
					slug: "my-usual-team",
					template: { ...PAIR.template, name: "My usual team" },
				},
			],
			unreadable: [],
		});
		await screen.findByText("My usual team", { selector: "strong" });
		for (const line of en.savedTeamsLead.split("\n")) onlyInTips(line);
		cleanup();

		const computer = await settings(TEAM, "/settings/computer");
		await answerStatus(computer.s, false, 2);
		await screen.findByText("/home/me/corner-bakery");
		for (const line of en.connectedBody.split("\n")) onlyInTips(line);
		const connected = screen.getByText(en.connectedLead);
		expect(connected.closest('[role="tooltip"]')).toBeNull();
	});
});
