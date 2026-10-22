import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { en } from "../../strings/en.ts";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { media } from "../../test/media.ts";
import {
	answerQuery,
	answerStatus,
	renderApp,
} from "../../test/render-app.tsx";
import { onlyInTips } from "../../test/tips.ts";
import { SECTIONS } from "./SettingsLayout.tsx";

const WIDE = "(min-width: 1024px)";

const agent = (role: string, status = "active") => ({
	id: role,
	display_name: role,
	role,
	status,
});
/** A team as `team.get` answers it, with or without a Designer. */
const team = (...agents: ReturnType<typeof agent>[]) => ({
	team: { agents },
	agents: [],
	max_agents: 7,
});
const WITH_DESIGNER = team(
	agent("software_developer"),
	agent("ui_ux_designer"),
);
const WITHOUT = team(agent("software_developer"));

/** Settings at `path`, the shell and the layout answered. */
async function open(path: string, answered: object | null = WITH_DESIGNER) {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	// The layout asks for the team only to list "Your app".
	if (answered) await answerQuery(s, "team.get", answered);
	return { container, s };
}

const main = () => within(screen.getByRole("main"));

describe("settings directory", () => {
	afterEach(() => localStorage.clear());

	it("opens_general_beside_the_list_on_a_wide_screen", async () => {
		media.set(WIDE, true);
		const { container } = await open("/settings");
		expect(
			await screen.findByRole("heading", { name: en.sectionGeneral }),
		).toBeTruthy();
		const list = await screen.findByRole("navigation", {
			name: en.settingsSections,
		});
		await waitFor(() =>
			expect(within(list).getAllByRole("link")).toHaveLength(SECTIONS.length),
		);
		const links = within(list).getAllByRole("link");
		// The design's order and addresses, not merely SECTIONS' own.
		expect(links.map((a) => [a.textContent, a.getAttribute("href")])).toEqual([
			[en.sectionGeneral, "/t/shop/settings/general"],
			[en.sectionAllowedActions, "/t/shop/settings/allowed-actions"],
			[en.sectionRules, "/t/shop/settings/rules"],
			[en.sectionSpending, "/t/shop/settings/spending"],
			[en.sectionWork, "/t/shop/settings/work"],
			[en.sectionYourApp, "/t/shop/settings/your-app"],
			[en.sectionSavedTeams, "/t/shop/settings/saved-teams"],
			[en.sectionComputer, "/t/shop/settings/computer"],
			[en.sectionAccount, "/t/shop/settings/account"],
		]);
		// The team's rules have four sections of their own, not one.
		expect(links).toHaveLength(9);
		expect(links.map((a) => a.textContent)).not.toContain("Your team’s rules");
		expect(links.map((a) => a.getAttribute("aria-current"))).toEqual(
			SECTIONS.map((s) => (s.path === "general" ? "page" : null)),
		);
		// The rail's own "Settings" link is outside main.
		expect(main().queryByRole("link", { name: en.settingsBack })).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("shows_the_list_alone_on_a_phone", async () => {
		const { container } = await open("/settings");
		const list = await screen.findByRole("navigation", {
			name: en.settingsSections,
		});
		await waitFor(() =>
			expect(within(list).getAllByRole("link")).toHaveLength(SECTIONS.length),
		);
		for (const s of SECTIONS) {
			const row = within(list).getByRole("link", {
				name: new RegExp(`^${en[s.name]}`),
			});
			expect(row.textContent).toContain(en[s.summary]);
		}
		expect(
			screen.queryByRole("heading", { name: en.sectionGeneral }),
		).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("puts_a_way_back_above_a_section_on_a_phone", async () => {
		await open("/settings/general", null);
		const back = await main().findByRole("link", { name: en.settingsBack });
		expect(back.getAttribute("href")).toBe("/t/shop/settings");
		const heading = main().getByRole("heading", { name: en.sectionGeneral });
		expect(
			back.compareDocumentPosition(heading) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		act(() => media.set(WIDE, true));
		expect(main().queryByRole("link", { name: en.settingsBack })).toBeNull();
	});

	it("leaves_your_app_out_without_a_designer", async () => {
		media.set(WIDE, true);
		for (const none of [WITHOUT, team(agent("ui_ux_designer", "retired"))]) {
			const { s } = await open("/settings/your-app", none);
			expect(await screen.findByText(en.yourAppNone)).toBeTruthy();
			expect(
				main().getByRole("link", { name: en.team }).getAttribute("href"),
			).toBe("/t/shop/team");
			const list = screen.getByRole("navigation", {
				name: en.settingsSections,
			});
			expect(
				within(list).queryByRole("link", { name: en.sectionYourApp }),
			).toBeNull();
			expect(s).toBeTruthy();
			cleanup();
		}
		await open("/settings/your-app");
		const list = await screen.findByRole("navigation", {
			name: en.settingsSections,
		});
		expect(
			await within(list).findByRole("link", { name: en.sectionYourApp }),
		).toBeTruthy();
		expect(
			await screen.findByRole("region", { name: en.previewTitle }),
		).toBeTruthy();
		expect(screen.queryByText(en.yourAppNone)).toBeNull();
	});

	it("shows_not_found_for_an_unknown_section", async () => {
		const { s } = await open("/settings/nope", null);
		expect(s).toBeTruthy();
		expect(
			await screen.findByRole("heading", { name: en.noPage }),
		).toBeTruthy();
	});

	it("takes_an_old_preview_link_to_your_app", async () => {
		await open("/settings#preview");
		expect(
			await screen.findByRole("region", { name: en.previewTitle }),
		).toBeTruthy();
		cleanup();

		media.set(WIDE, true);
		await open("/settings#preview");
		expect(
			await screen.findByRole("region", { name: en.previewTitle }),
		).toBeTruthy();
		const list = screen.getByRole("navigation", { name: en.settingsSections });
		const link = await within(list).findByRole("link", {
			name: en.sectionYourApp,
		});
		expect(link.getAttribute("aria-current")).toBe("page");
	});

	it("holds_the_theme_and_the_language_in_general", async () => {
		await open("/settings/general", null);
		expect(await screen.findByRole("radio", { name: /^Dark/ })).toBeTruthy();
		expect(screen.getByText(en.english)).toBeTruthy();
		expect(
			screen.queryByRole("switch", { name: en.advancedSwitch }),
		).toBeNull();
	});

	it("splits_the_rules_across_three_sections", async () => {
		const rules = {
			team: {
				name: "Corner Bakery",
				agents: [agent("software_developer")],
				budgets: {},
				policy: {
					integration: "auto_merge",
					permissions: { run_commands: false, push: false },
				},
				rules: {},
			},
			agents: [],
			max_agents: 7,
		};
		const defaults = {
			budgets: {},
			rules: {},
			policy: {
				integration: "auto_merge",
				permissions: { run_commands: true, push: false },
			},
			ui_paths: ["**/*.tsx"],
		};
		const parts = [
			en.rulesMay,
			en.rulesSpendTitle,
			en.rulesFinish,
			en.rulesBranch,
			en.rulesDocuments,
			en.uiPathsTitle,
		];
		const pages = [
			["allowed-actions", en.sectionAllowedActions, [en.rulesMay]],
			["spending", en.sectionSpending, [en.rulesSpendTitle]],
			[
				"work",
				en.sectionWork,
				[en.rulesFinish, en.rulesBranch, en.rulesDocuments],
			],
		] as const;
		for (const [path, name, held] of pages) {
			const { container, s } = await open(`/settings/${path}`, rules);
			await answerQuery(s, "settings.defaults", defaults);
			expect(
				await screen.findByRole("heading", { level: 2, name }),
			).toBeTruthy();
			// The section's name is the page's one h2; each part beneath it is an h3.
			expect(
				main()
					.getAllByRole("heading", { level: 2 })
					.map((h) => h.textContent),
			).toEqual([name]);
			for (const part of held)
				expect(
					main().getByRole("heading", { level: 3, name: part }),
				).toBeTruthy();
			expect(
				main()
					.getAllByRole("heading", { level: 3 })
					.map((h) => h.textContent),
			).toEqual(held);
			for (const part of parts) {
				const region = main().queryByRole("region", { name: part });
				// The patterns come with Advanced, not with the page.
				if ((held as readonly string[]).includes(part))
					expect(region).toBeTruthy();
				else expect(region).toBeNull();
			}
			for (const line of en.rulesNextSession.split("\n")) onlyInTips(line);
			await expectNoAxeViolations(container);
			cleanup();
		}

		const { s } = await open("/settings/work", rules);
		await answerQuery(s, "settings.defaults", defaults);
		await main().findByRole("heading", { level: 3, name: en.rulesDocuments });
		fireEvent.click(main().getByRole("button", { name: en.advancedSwitch }));
		expect(
			await main().findByRole("region", { name: en.uiPathsTitle }),
		).toBeTruthy();
	});

	it("asks_before_letting_catervas_merge", async () => {
		const defaults = {
			budgets: {},
			rules: {},
			policy: {},
			ui_paths: ["**/*.tsx"],
		};
		const rules = (mergingAllowed: boolean) => ({
			team: {
				name: "Corner Bakery",
				agents: [agent("software_developer")],
				budgets: {},
				policy: { integration: "pull_request" },
				rules: {},
			},
			agents: [],
			max_agents: 7,
			online: true,
			merging_allowed: mergingAllowed,
		});
		const calls = (s: FakeSocket) => s.calls("merging.set");

		const { s } = await open("/settings/work", rules(false));
		await answerQuery(s, "settings.defaults", defaults);
		const toggle = await main().findByRole("switch", {
			name: en.mergingSwitch,
		});
		// 1. The warning comes first, and nothing is sent.
		fireEvent.click(toggle);
		const dialog = await screen.findByRole("dialog", {
			name: en.mergingWarnTitle,
		});
		expect(
			within(dialog).getByText("You won't see the work before it lands."),
		).toBeTruthy();
		expect(calls(s)).toHaveLength(0);

		// 2. Cancel closes it, still with nothing sent.
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.mergingCancel }),
		);
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(calls(s)).toHaveLength(0);

		// 3. Turn on sends the confirmation.
		fireEvent.click(toggle);
		fireEvent.click(
			within(
				await screen.findByRole("dialog", { name: en.mergingWarnTitle }),
			).getByRole("button", { name: en.mergingTurnOn }),
		);
		const frame = await waitFor(() => {
			const f = calls(s)[0];
			if (!f) throw new Error("no merging.set was sent");
			return f;
		});
		expect(frame.params).toEqual({ on: true, confirmed: true });
		cleanup();

		// 4. Turning it off needs no warning.
		const on = await open("/settings/work", rules(true));
		await answerQuery(on.s, "settings.defaults", defaults);
		fireEvent.click(
			await main().findByRole("switch", { name: en.mergingSwitch }),
		);
		const off = await waitFor(() => {
			const f = calls(on.s)[0];
			if (!f) throw new Error("no merging.set was sent");
			return f;
		});
		expect(off.params).toEqual({ on: false });
		expect(screen.queryByRole("dialog")).toBeNull();
	});

	it("turns_autopilot_on_only_from_the_dialog_and_with_a_daily_limit", async () => {
		const defaults = { budgets: {}, rules: {}, policy: {}, ui_paths: [] };
		const rules = (budgets: object, autopilot = false) => ({
			team: {
				name: "Corner Bakery",
				agents: [agent("software_developer")],
				budgets,
				policy: { integration: "pull_request" },
				rules: {},
			},
			agents: [],
			max_agents: 7,
			autopilot,
		});
		const calls = (s: FakeSocket) => s.calls("autopilot.set");
		const path = "/settings/allowed-actions";

		// 1. With no daily limit the switch is off limits, and the Spending link says why.
		const none = await open(path, rules({}));
		await answerQuery(none.s, "settings.defaults", defaults);
		const locked = await main().findByRole("switch", {
			name: en.autopilotSwitch,
		});
		expect(locked).toHaveProperty("disabled", true);
		expect(
			main()
				.getByRole("link", { name: en.autopilotNeedsLimitLink })
				.getAttribute("href"),
		).toBe("/t/shop/settings/spending");
		cleanup();

		// 2. With a limit, pressing the switch sends nothing and opens the dialog.
		const { s } = await open(path, rules({ daily_usd: 5 }));
		await answerQuery(s, "settings.defaults", defaults);
		const toggle = await main().findByRole("switch", {
			name: en.autopilotSwitch,
		});
		expect(toggle).toHaveProperty("disabled", false);
		fireEvent.click(toggle);
		const dialog = await screen.findByRole("dialog", {
			name: en.autopilotWarnTitle,
		});
		const lines = en.autopilotWarn.split("\n");
		expect(lines).toHaveLength(8);
		for (const line of lines)
			expect(
				within(dialog).getByText(
					line.replace("{limit}", "5").replace("{perHour}", "30"),
				),
			).toBeTruthy();
		expect(within(dialog).getByText(/daily limit of \$5 a day/)).toBeTruthy();
		expect(within(dialog).getByText(/30 paid tool calls/)).toBeTruthy();
		expect(calls(s)).toHaveLength(0);

		// 3. Cancel closes it, still with nothing sent.
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.autopilotCancel }),
		);
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(calls(s)).toHaveLength(0);

		// 4. Turn on sends the confirmation; a refusal is said in the alert.
		fireEvent.click(toggle);
		fireEvent.click(
			within(
				await screen.findByRole("dialog", { name: en.autopilotWarnTitle }),
			).getByRole("button", { name: en.autopilotTurnOn }),
		);
		const frame = await waitFor(() => {
			const f = calls(s)[0];
			if (!f) throw new Error("no autopilot.set was sent");
			return f;
		});
		expect(frame.params).toEqual({ on: true, confirmed: true });
		await s.fail(frame, -32005, "refused", {
			errors: [
				{
					path: "/budgets/daily_usd",
					message: "Autopilot needs a daily spending limit.",
					code: "daily_limit_required",
				},
			],
		});
		expect((await screen.findByRole("alert")).textContent).toContain(
			en.refuseDailyLimitRequired,
		);
		cleanup();

		// 5. Turning it off needs no dialog.
		const on = await open(path, rules({ daily_usd: 5 }, true));
		await answerQuery(on.s, "settings.defaults", defaults);
		const live = await main().findByRole("switch", {
			name: en.autopilotSwitch,
		});
		expect(live.getAttribute("aria-checked")).toBe("true");
		fireEvent.click(live);
		const off = await waitFor(() => {
			const f = calls(on.s)[0];
			if (!f) throw new Error("no autopilot.set was sent");
			return f;
		});
		expect(off.params).toEqual({ on: false });
		expect(screen.queryByRole("dialog")).toBeNull();
	});

	it("opens_work's_advanced_settings_under_its_button", async () => {
		const defaults = {
			budgets: {},
			rules: {},
			policy: {},
			ui_paths: ["**/*.tsx"],
		};
		const rules = {
			team: {
				name: "Corner Bakery",
				agents: [],
				budgets: {},
				policy: {},
				rules: {},
			},
			agents: [],
			max_agents: 7,
		};
		const { container, s } = await open("/settings/work", rules);
		await answerQuery(s, "settings.defaults", defaults);
		const button = await main().findByRole("button", {
			name: en.advancedSwitch,
		});
		expect(main().queryByRole("region", { name: en.uiPathsTitle })).toBeNull();
		await expectNoAxeViolations(container);

		fireEvent.click(button);
		const list = await main().findByRole("list", { name: en.advanced });
		expect(
			within(list).getByRole("region", { name: en.uiPathsTitle }),
		).toBeTruthy();
		expect(main().getByRole("button", { name: en.advancedHide })).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(main().getByRole("button", { name: en.advancedHide }));
		expect(main().queryByRole("list", { name: en.advanced })).toBeNull();
		expect(main().queryByRole("region", { name: en.uiPathsTitle })).toBeNull();
	});

	it("edits_the_sandbox_tools", async () => {
		const rules = {
			team: {
				name: "Corner Bakery",
				agents: [],
				budgets: {},
				policy: {},
				rules: {},
			},
			agents: [],
			max_agents: 7,
		};
		const defaults = {
			budgets: {},
			rules: {},
			policy: {},
			ui_paths: ["**/*.tsx"],
		};
		const { s } = await open("/settings/work", rules);
		await answerQuery(s, "settings.defaults", defaults);
		fireEvent.click(
			await main().findByRole("button", { name: en.advancedSwitch }),
		);
		const region = await main().findByRole("region", {
			name: en.sandboxToolsTitle,
		});
		fireEvent.change(within(region).getByLabelText(en.sandboxToolsLabel), {
			target: { value: "ffmpeg\nlibnss3\n" },
		});
		fireEvent.click(within(region).getByRole("button", { name: en.agentSave }));
		const save = await waitFor(() => {
			const f = s.calls("team.save").at(-1);
			if (!f) throw new Error("no team.save was sent");
			return f;
		});
		const sent = (save.params as { team: { sandbox?: unknown } }).team;
		expect(sent.sandbox).toEqual({ packages: ["ffmpeg", "libnss3"] });
	});

	it("keeps_advanced_settings_to_their_own_section", async () => {
		localStorage.setItem("catervas.advanced", "true");
		const rules = {
			team: {
				name: "Corner Bakery",
				agents: [],
				budgets: {},
				policy: {},
				rules: {},
			},
			agents: [],
			max_agents: 7,
		};
		const defaults = {
			budgets: {},
			rules: {},
			policy: {},
			ui_paths: ["**/*.tsx"],
		};
		const { s } = await open("/settings/work", rules);
		await answerQuery(s, "settings.defaults", defaults);
		await main().findByRole("button", { name: en.advancedSwitch });
		expect(main().queryByRole("region", { name: en.uiPathsTitle })).toBeNull();
		cleanup();

		// The rules sections are checked once their parts have loaded, not at their title.
		const loaded: Record<string, string> = {
			"allowed-actions": en.rulesMay,
			spending: en.rulesSpendTitle,
		};
		for (const path of [
			"general",
			"allowed-actions",
			"spending",
			"your-app",
			"saved-teams",
			"computer",
			"account",
		]) {
			const part = loaded[path];
			const { s } = await open(
				`/settings/${path}`,
				part ? rules : path === "your-app" ? WITH_DESIGNER : null,
			);
			await screen.findByRole("heading", { level: 2 });
			if (part) {
				await answerQuery(s, "settings.defaults", defaults);
				await main().findByRole("heading", { level: 3, name: part });
			}
			expect(
				main().queryByRole("button", { name: en.advancedSwitch }),
			).toBeNull();
			expect(
				main().queryByRole("switch", { name: en.advancedSwitch }),
			).toBeNull();
			cleanup();
		}
	});

	describe("rules", () => {
		const rulesTeam = (policy: object = {}, rules: object = {}) => ({
			team: {
				name: "Corner Bakery",
				agents: [agent("software_developer")],
				budgets: {},
				policy: { integration: "auto_merge", ...policy },
				rules,
			},
			agents: [],
			max_agents: 7,
		});
		const defaults = { budgets: {}, rules: {}, policy: {}, ui_paths: [] };
		/** The Rules page, its team answered. */
		async function rulesPage(team = rulesTeam()) {
			const { s } = await open("/settings/rules", team);
			await answerQuery(s, "settings.defaults", defaults);
			await main().findByRole("heading", {
				level: 3,
				name: en.rulesFilesTitle,
			});
			return s;
		}
		/** The team the last Save sent, in wire form. */
		const saved = async (s: FakeSocket) => {
			const frame = await waitFor(() => {
				const f = s.calls("team.save")[0];
				if (!f) throw new Error("no team.save was sent");
				return f;
			});
			return frame.params.team as Record<string, unknown>;
		};
		const row = (glob: string) =>
			main().getByRole("group", {
				name: new RegExp(`\\(${glob.replace(/[.*]/g, "\\$&")}\\)$`),
			});

		it("lifts_a_shipped_path_makes_it_read_only_and_adds_the_owners_own", async () => {
			const s = await rulesPage();
			fireEvent.click(
				within(row(".env")).getByRole("radio", { name: en.filesReadOnly }),
			);
			expect(main().getByText(en.forbiddenSecretsWarn)).toBeTruthy();
			fireEvent.change(main().getByLabelText(en.filesAdd), {
				target: { value: "infra/**" },
			});
			fireEvent.click(
				partOf(en.rulesFilesTitle).getByRole("button", {
					name: en.filesAddButton,
				}),
			);
			await saveOf(partOf(en.rulesFilesTitle));
			expect((await saved(s)).rules).toEqual({
				lifted_paths: [".env"],
				read_only_paths: [".env"],
				protected_paths: ["infra/**"],
			});
		});

		/** The part under `title`, its Save button and the words typed into it. */
		const partOf = (title: string) =>
			within(main().getByRole("region", { name: title }));
		const typeInto = (
			part: ReturnType<typeof partOf>,
			label: string,
			v: string,
		) => fireEvent.change(part.getByLabelText(label), { target: { value: v } });
		const saveOf = async (part: ReturnType<typeof partOf>) => {
			const save = await part.findByRole("button", { name: en.agentSave });
			await waitFor(() => expect(save).toHaveProperty("disabled", false));
			fireEvent.click(save);
		};

		it("adds_a_forbidden_command_in_plain_words_and_git_the_owner_allows", async () => {
			const s = await rulesPage();
			const commands = partOf(en.rulesCommandsTitle);
			typeInto(commands, en.commandsAdd, "rm -rf");
			fireEvent.click(
				commands.getByRole("button", { name: en.commandsAddButton }),
			);
			expect(
				commands.getByRole("button", { name: `${en.commandsRemove} rm -rf` }),
			).toBeTruthy();
			await saveOf(commands);
			expect((await saved(s)).rules).toEqual({
				forbidden_commands: ["^rm -rf"],
			});
			cleanup();

			const again = await rulesPage();
			const git = partOf(en.rulesGitTitle);
			// The read-only eight are always allowed, and cannot be taken away.
			for (const word of [
				"status",
				"diff",
				"log",
				"show",
				"ls-files",
				"grep",
				"rev-parse",
				"blame",
			]) {
				expect(git.getByText(word)).toBeTruthy();
				expect(
					git.queryByRole("button", { name: `${en.gitRemove} ${word}` }),
				).toBeNull();
			}
			typeInto(git, en.gitAdd, "Fetch");
			expect(git.getByRole("button", { name: en.gitAddButton })).toHaveProperty(
				"disabled",
				true,
			);
			expect(git.getByText(en.gitAddWrong)).toBeTruthy();
			typeInto(git, en.gitAdd, "fetch");
			fireEvent.click(git.getByRole("button", { name: en.gitAddButton }));
			await saveOf(git);
			expect((await saved(again)).rules).toEqual({
				git_subcommands: ["fetch"],
			});
		});

		it("lists_the_remotes_agents_may_push_to", async () => {
			const s = await rulesPage(
				rulesTeam({ permissions: { run_commands: false, push: true } }),
			);
			const remotes = partOf(en.rulesRemotesTitle);
			// The last remote cannot be removed.
			expect(
				remotes.getByRole("button", {
					name: new RegExp(`^${en.remotesRemove} origin`),
				}),
			).toHaveProperty("disabled", true);
			typeInto(remotes, en.remotesAdd, "fork");
			fireEvent.click(
				remotes.getByRole("button", { name: en.remotesAddButton }),
			);
			await saveOf(remotes);
			expect(
				((await saved(s)).policy as Record<string, unknown>).push_remotes,
			).toEqual(["origin", "fork"]);
			cleanup();

			// With no agent able to push, the part is one line pointing to Allowed actions.
			await rulesPage();
			const none = main().getByRole("region", { name: en.rulesRemotesTitle });
			expect(within(none).queryByLabelText(en.remotesAdd)).toBeNull();
			expect(
				within(none)
					.getByRole("link", { name: en.sectionAllowedActions })
					.getAttribute("href"),
			).toBe("/t/shop/settings/allowed-actions");
		});

		it("sets_when_work_stops_and_how_long_a_session_runs", async () => {
			const s = await rulesPage();
			const limits = partOf(en.rulesLimitsTitle);
			typeInto(limits, en.limitsReadiness, "5");
			await saveOf(limits);
			expect(
				((await saved(s)).policy as Record<string, unknown>).readiness_attempts,
			).toBe(5);
			cleanup();

			const rate = await rulesPage();
			const paid = partOf(en.rulesLimitsTitle);
			typeInto(paid, en.limitsPaidCalls, "1001");
			expect(
				within(
					main()
						.getByLabelText(en.limitsPaidCalls)
						.closest("div") as HTMLElement,
				).getByText(
					en.limitsWrong.replace("{min}", "1").replace("{max}", "1000"),
				),
			).toBeTruthy();
			typeInto(paid, en.limitsPaidCalls, "12");
			await saveOf(paid);
			expect(
				((await saved(rate)).policy as Record<string, unknown>)
					.paid_calls_per_hour,
			).toBe(12);
			cleanup();

			const again = await rulesPage();
			const session = partOf(en.rulesSessionTitle);
			typeInto(session, en.sessionMinutes, "10");
			await saveOf(session);
			expect(
				((await saved(again)).budgets as { session: unknown }).session,
			).toEqual({ max_wall_clock_seconds: 600 });
			cleanup();

			// Put back clears each from the saved team.
			const set = rulesTeam({ readiness_attempts: 5 });
			set.team.budgets = { session: { max_wall_clock_seconds: 600 } } as never;
			const back = await rulesPage(set);
			const lim = partOf(en.rulesLimitsTitle);
			fireEvent.click(lim.getByRole("button", { name: en.putBack }));
			await saveOf(lim);
			expect(
				((await saved(back)).policy as Record<string, unknown>)
					.readiness_attempts,
			).toBeUndefined();
			cleanup();

			const back2 = await rulesPage(set);
			const ses = partOf(en.rulesSessionTitle);
			fireEvent.click(ses.getByRole("button", { name: en.putBack }));
			await saveOf(ses);
			expect(
				((await saved(back2)).budgets as Record<string, unknown>).session,
			).toBeUndefined();
		});

		it("keeps_catervass_own_files_unwritable", async () => {
			await rulesPage();
			const own = within(row(".catervas/local/**"));
			expect(
				own.getAllByRole("radio").map((r) => r.getAttribute("value")),
			).toEqual(["off", "read"]);
			expect(own.queryByRole("radio", { name: en.filesAllowed })).toBeNull();
			// The other four offer all three answers.
			expect(
				within(row(".env")).getByRole("radio", { name: en.filesAllowed }),
			).toBeTruthy();
		});
	});
});
