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
	},
};

/** Settings, with the team and the defaults answered: its team's rules area. */
async function settings() {
	const { container, socket } = await renderApp("/settings");
	const s = socket as FakeSocket;
	// The shell asks first, then the page itself.
	await answerStatus(s, false);
	await answerStatus(s, false, 2);
	await answerQuery(s, "team.get", {
		team: TEAM,
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
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("button", { name: "Pause Theo" }));
		expect((await sent(s, "command")).params).toEqual({
			command: {
				command: "agent_update",
				body: { agent_id: "theo", status: "paused" },
			},
		});
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
		// The cost section says what a sixth agent, and its plans, add.
		expect(
			screen.getByText(
				"Iris uses the same model as Theo, so a sixth agent adds to what a day costs. Before Iris changes a screen, Iris looks at it and writes a plan, and that costs a little too.",
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
		const checked = await validated(s, ["Theo now thinks with low effort."]);
		expect(one(checked, "theo").model).toEqual({
			id: "claude-opus-5-5",
			effort: "low",
		});
		expect(
			await screen.findByText("Theo now thinks with low effort."),
		).toBeTruthy();
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
		// The daemon's refusal is the sentence to show.
		const wrong = await sent(s, "account.connect");
		await s.fail(
			wrong,
			-32005,
			"that is not a subscription key: it starts with sk-ant-oat",
		);
		expect(
			await within(row).findByText(
				"that is not a subscription key: it starts with sk-ant-oat",
			),
		).toBeTruthy();
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
