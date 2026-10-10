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
	judges: [],
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
			[en.sectionGeneral, "/settings/general"],
			[en.sectionAllowedActions, "/settings/allowed-actions"],
			[en.sectionSpending, "/settings/spending"],
			[en.sectionWork, "/settings/work"],
			[en.sectionYourApp, "/settings/your-app"],
			[en.sectionSavedTeams, "/settings/saved-teams"],
			[en.sectionComputer, "/settings/computer"],
			[en.sectionAccount, "/settings/account"],
		]);
		// The team's rules have three sections of their own, not one.
		expect(links).toHaveLength(8);
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
		expect(back.getAttribute("href")).toBe("/settings");
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
			).toBe("/team");
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

	it("holds_the_theme_the_language_and_advanced_in_general", async () => {
		await open("/settings/general", null);
		expect(await screen.findByRole("radio", { name: /^Dark/ })).toBeTruthy();
		expect(screen.getByText(en.english)).toBeTruthy();
		const advanced = screen.getByRole("switch", { name: en.advancedSwitch });
		expect(advanced).toBeTruthy();
		for (const line of en.advancedMore.split("\n")) onlyInTips(line);
		fireEvent.click(advanced);
		expect(localStorage.getItem("catervas.advanced")).toBe("true");
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
			judges: [],
			max_agents: 7,
		};
		const defaults = {
			budgets: {},
			rules: {},
			policy: {
				integration: "auto_merge",
				judgment: {
					required: "always",
					questions: ["Does the task fit its budget?"],
					judge: "auto",
				},
				permissions: { run_commands: true, push: false },
				plan_in_sprints: true,
			},
			ui_paths: ["**/*.tsx"],
		};
		const parts = [
			en.rulesMay,
			en.rulesSpendTitle,
			en.rulesFinish,
			en.rulesPlanning,
			en.planTitle,
			en.uiPathsTitle,
		];
		const pages = [
			["allowed-actions", en.sectionAllowedActions, [en.rulesMay]],
			["spending", en.sectionSpending, [en.rulesSpendTitle]],
			[
				"work",
				en.sectionWork,
				[en.rulesFinish, en.rulesPlanning, en.planTitle],
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

		localStorage.setItem("catervas.advanced", "true");
		const { s } = await open("/settings/work", rules);
		await answerQuery(s, "settings.defaults", defaults);
		expect(
			await main().findByRole("region", { name: en.uiPathsTitle }),
		).toBeTruthy();
	});
});
