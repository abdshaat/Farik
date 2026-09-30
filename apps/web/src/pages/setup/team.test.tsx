import { AVATAR_URLS } from "@farik/ui";
import { expectNoAxeViolations } from "@farik/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import type { FakeSocket } from "../../test/fake-socket.ts";
import {
	answerQuery,
	answerStatus,
	renderApp,
} from "../../test/render-app.tsx";

const BUDGET = "Does the task fit its budget?";
const NOTICE =
	"Would its checks notice if the work went wrong the way its intent worries about?";
const SMALL = "Is it small enough to finish in one go?";

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	persona: `${name} persona`,
	status: "active",
	model: { id: "claude-opus-5", effort: "high" },
});
const FIVE = [
	agent("mira", "Mira", "product_manager", "product-manager"),
	agent("sol", "Sol", "scrum_master", "scrum-master"),
	agent("ada", "Ada", "architect", "architect"),
	agent("theo", "Theo", "software_developer", "developer"),
	agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
];
const IRIS = agent("iris", "Iris", "ui_ux_designer", "extra-1");
const TESTS_PASS = {
	name: "the-tests-pass",
	text: "Every test passes: pnpm test.",
	source: "project_scan",
	verification: { method: "test", command: "pnpm test" },
};

function proposed(change: (team: Record<string, unknown>) => void = () => {}) {
	const team: Record<string, unknown> = {
		name: "Corner Bakery",
		agents: FIVE,
		budgets: {},
		policy: {
			human_accepts_contracts: "high_risk",
			wip_limit_per_agent: 1,
			blocked_limit_hours: 24,
			max_iterations: 3,
			integration: "auto_merge",
			judgment: {
				required: "always",
				questions: [BUDGET, NOTICE],
				judge: "auto",
			},
			permissions: { run_commands: true, push: false },
		},
		rules: { require_new_tests: false },
	};
	change(team);
	return { team, criteria: { criteria: [TESTS_PASS] } };
}

/** The latest request of `method` the page sent. */
async function sent(socket: FakeSocket, method: string) {
	return waitFor(() => {
		const frame = socket.calls(method).at(-1);
		if (!frame) throw new Error(`no ${method} was sent`);
		return frame;
	});
}

/** The params of the latest `name` query the page asked. */
async function asked(socket: FakeSocket, name: string) {
	return waitFor(() => {
		const frame = socket
			.calls("query")
			.filter((f) => f.params.name === name)
			.at(-1);
		if (!frame) throw new Error(`no ${name} was asked`);
		return frame;
	});
}

/** Presses Continue on each screen from spending to finishing, then Start the team. */
async function startFrom(socket: FakeSocket, screenTitle: string) {
	const order = [en.mayTitle, en.spendTitle, en.finishTitle];
	for (const title of order.slice(order.indexOf(screenTitle))) {
		await screen.findByRole("heading", { name: title });
		if (title === en.finishTitle) break;
		fireEvent.click(screen.getByRole("button", { name: en.continue }));
	}
	fireEvent.click(await screen.findByRole("button", { name: en.startTeam }));
	return (await sent(socket, "team.start")).params as {
		team: Record<string, never>;
		criteria: { criteria: unknown[] };
	};
}

describe("team setup", () => {
	it("sends_a_new_project_to_the_scan", async () => {
		const first = await renderApp("/");
		await answerStatus(first.socket as FakeSocket, true, 1, {
			setup_pending: true,
		});
		expect(
			await screen.findByRole("heading", { name: en.scanTitle }),
		).toBeTruthy();
		cleanup();

		const second = await renderApp("/");
		await answerStatus(second.socket as FakeSocket, false, 1, {
			setup_pending: false,
		});
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
	});

	it("reads_the_scan_back_in_rows", async () => {
		const { container, socket } = await renderApp("/setup/scan");
		const s = socket as FakeSocket;
		await answerQuery(s, "project.scan", {
			facts: {
				language: "TypeScript",
				toolchain: "pnpm",
				workspace: true,
				packages: 3,
				tests_in: "vitest",
				tracked_files: 120,
				last_commit: "4 days ago",
			},
			checks: ["Every test passes: pnpm test.", "The style check passes."],
			kept_private: [".env", "**/*.key"],
		});
		const row = async (term: string) => {
			const dt = await screen.findByText(term, { selector: "dt" });
			return dt.nextElementSibling?.textContent;
		};
		expect(await row(en.scanWhat)).toBe(
			"TypeScript, pnpm, a workspace of 3 packages",
		);
		expect(await row(en.scanTested)).toBe("Tested with vitest");
		expect(await row(en.scanChecked)).toContain(
			"Every test passes: pnpm test.",
		);
		expect(await row(en.scanChecked)).toContain("The style check passes.");
		expect(await row(en.scanLast)).toBe("4 days ago");
		expect(await row(en.scanPrivate)).toBe(
			`.env, **/*.key. ${en.scanPrivateNote}`,
		);
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("button", { name: en.scanWrong }));
		fireEvent.change(screen.getByLabelText(en.scanWrongField), {
			target: { value: "It is a shop, not a game." },
		});
		fireEvent.click(screen.getByRole("button", { name: en.scanWrongSave }));
		const note = await sent(s, "project.note");
		expect(note.params).toEqual({ text: "It is a shop, not a game." });
		await s.reply(note, {});
		expect(await screen.findByText(en.scanWrongSaved)).toBeTruthy();

		// One package is no workspace to speak of.
		cleanup();
		const one = await renderApp("/setup/scan");
		await answerQuery(one.socket as FakeSocket, "project.scan", {
			facts: {
				language: "TypeScript",
				toolchain: "pnpm",
				workspace: true,
				packages: 1,
				tests_in: null,
				tracked_files: 3,
				last_commit: null,
			},
			checks: [],
			kept_private: [],
		});
		expect(await row(en.scanWhat)).toBe("TypeScript, pnpm");
		const t2 = one.socket as FakeSocket;
		fireEvent.click(screen.getByRole("button", { name: en.scanRight }));
		await answerQuery(t2, "team.propose", proposed());
		expect(
			await screen.findByRole("heading", { name: en.teamTitle }),
		).toBeTruthy();
	});

	it("builds_the_team_from_the_five", async () => {
		const { container, socket } = await renderApp("/setup/team");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		const list = await screen.findByRole("list", { name: en.teamMembers });
		expect(within(list).getAllByRole("listitem")).toHaveLength(5);
		expect(within(list).getByText("Ada persona")).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(
			screen.getByRole("checkbox", { name: "Include Scrum Master" }),
		);
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		const names = within(list).getAllByRole("textbox", {
			name: "Name for the Developer",
		});
		expect(names.map((n) => (n as HTMLInputElement).value)).toEqual([
			"Theo",
			"Noor",
		]);
		// A name whose id is taken gets the next free one.
		fireEvent.change(
			screen.getByRole("textbox", {
				name: "Name for the Marketing Specialist",
			}),
			{ target: { value: "Theo" } },
		);
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		const validate = await asked(s, "team.validate");
		const team = (validate.params.params as { team: { agents: never[] } }).team;
		expect(
			team.agents.map(
				(a: { id: string; display_name: string; role: string }) => [
					a.id,
					a.display_name,
					a.role,
				],
			),
		).toEqual([
			["mira", "Mira", "product_manager"],
			["ada", "Ada", "architect"],
			["theo", "Theo", "software_developer"],
			["theo-2", "Theo", "marketing_specialist"],
			["noor", "Noor", "software_developer"],
		]);
		await s.reply(validate, {
			errors: [
				{
					path: "/agents/1/display_name",
					message: '"" is shorter than 1 character',
					code: "name",
				},
			],
			effects: [],
		});
		// In plain words at its row, never the schema's own.
		const why = await screen.findByText(en.refuseName);
		expect(screen.queryByText(/shorter than/)).toBeNull();
		const ada = screen
			.getByRole("textbox", { name: "Name for the Architect" })
			.closest("li");
		expect(ada?.contains(why)).toBe(true);
		expect(screen.queryByRole("heading", { name: en.mayTitle })).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		await waitFor(() =>
			expect(
				s.calls("query").filter((f) => f.params.name === "team.validate"),
			).toHaveLength(2),
		);
		await s.reply(
			s
				.calls("query")
				.filter((f) => f.params.name === "team.validate")[1] as never,
			{ errors: [], effects: [] },
		);
		expect(
			await screen.findByRole("heading", { name: en.mayTitle }),
		).toBeTruthy();
	});

	it("offers_the_designer_among_the_six", async () => {
		const { container, socket } = await renderApp("/setup/team");
		const s = socket as FakeSocket;
		await answerQuery(
			s,
			"team.propose",
			proposed((team) => {
				team.agents = [...FIVE.slice(0, 4), IRIS, ...FIVE.slice(4)];
			}),
		);
		expect(await screen.findByText(en.teamLead)).toBeTruthy();
		expect(en.teamLead).toMatch(/^We suggest six, one for each job\./);
		const list = await screen.findByRole("list", { name: en.teamMembers });
		expect(within(list).getAllByRole("listitem")).toHaveLength(6);
		const include = screen.getByRole("checkbox", {
			name: "Include UI/UX Designer",
		}) as HTMLInputElement;
		expect(include.checked).toBe(true);
		expect(
			(
				screen.getByRole("textbox", {
					name: "Name for the UI/UX Designer",
				}) as HTMLInputElement
			).value,
		).toBe("Iris");
		const face = include.closest("li")?.querySelector("img") as HTMLElement;
		expect(face.getAttribute("src")).toBe(AVATAR_URLS["extra-1"]);
		expect(face.style.getPropertyValue("--ring")).toBe(
			"var(--farik-color-role-ui-ux-designer)",
		);
		await expectNoAxeViolations(container);

		// Added agents never take Iris's picture, nor the Finance Specialist's.
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		const faces = within(list)
			.getAllByRole("listitem")
			.slice(6)
			.map((row) => row.querySelector("img")?.getAttribute("src"));
		expect(faces.slice(0, 3)).toEqual([
			AVATAR_URLS["extra-2"],
			AVATAR_URLS["extra-3"],
			AVATAR_URLS["extra-5"],
		]);
		for (const taken of ["extra-1", "extra-4"] as const)
			expect(faces).not.toContain(AVATAR_URLS[taken]);
		for (const added of within(list).getAllByRole("checkbox").slice(6))
			fireEvent.click(added);

		expect(
			screen.getByRole("button", { name: "Continue with these six" }),
		).toBeTruthy();
		fireEvent.click(include);
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		const validate = await asked(s, "team.validate");
		const team = (validate.params.params as { team: { agents: never[] } }).team;
		expect(team.agents.map((a: { id: string }) => a.id)).toEqual([
			"mira",
			"sol",
			"ada",
			"theo",
			"kai",
		]);
	});

	it("adds_from_the_spare_names_without_mixing_rows_up", async () => {
		const warned = vi.spyOn(console, "error").mockImplementation(() => {});
		const { socket } = await renderApp("/setup/team");
		await answerQuery(socket as FakeSocket, "team.propose", proposed());
		const list = await screen.findByRole("list", { name: en.teamMembers });
		const developers = () =>
			within(list)
				.getAllByRole("textbox", { name: "Name for the Developer" })
				.map((n) => (n as HTMLInputElement).value);
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		expect(developers()).toEqual(["Theo", "Noor", "Ivo"]);
		// A renamed newcomer frees its spare name, which the next one takes.
		fireEvent.change(
			within(list).getAllByRole("textbox", {
				name: "Name for the Developer",
			})[1] as HTMLElement,
			{ target: { value: "Zed" } },
		);
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		expect(developers()).toEqual(["Theo", "Zed", "Ivo", "Noor"]);
		expect(
			warned.mock.calls.some((call) => String(call[0]).includes("same key")),
		).toBe(false);
		warned.mockRestore();
	});

	it("asks_both_permission_questions", async () => {
		const { container, socket } = await renderApp("/setup/permissions");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await screen.findByRole("heading", { name: en.mayTitle });
		const onward = () =>
			screen.getByRole("button", { name: en.continue }) as HTMLButtonElement;
		expect(onward().disabled).toBe(true);
		expect(screen.getByText(en.mayAnswerBoth)).toBeTruthy();
		// What each agent may do is the daemon's answer for the team as it stands.
		const theo = (tiers: string[]) => ({
			errors: [],
			effects: [],
			agents: [{ id: "theo", tiers }],
		});
		await s.reply(
			await asked(s, "team.validate"),
			theo(["read", "write_workspace", "execute", "git_local"]),
		);
		expect(
			await screen.findByText(
				"Reads the project, changes the files of its task, runs commands in a sealed box and saves its work on its own branch.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("radio", { name: /^No, nobody may/ }));
		expect(screen.getByText(en.mayStillChecks)).toBeTruthy();
		const noCommands = await waitFor(() => {
			const frame = s
				.calls("query")
				.filter((f) => f.params.name === "team.validate")
				.at(-1);
			if (!frame) throw new Error("no team.validate was asked");
			const team = (
				frame.params.params as {
					team: { policy: { permissions: { run_commands?: boolean } } };
				}
			).team;
			if (team.policy.permissions.run_commands !== false)
				throw new Error("the answer was not asked about yet");
			return frame as never;
		});
		await s.reply(noCommands, theo(["read", "write_workspace", "git_local"]));
		expect(
			await screen.findByText(
				"Reads the project, changes the files of its task and saves its work on its own branch.",
			),
		).toBeTruthy();
		expect(onward().disabled).toBe(true);
		fireEvent.click(
			screen.getByRole("radio", { name: /^No, keep everything/ }),
		);
		expect(onward().disabled).toBe(false);
		expect(screen.queryByText(en.mayAnswerBoth)).toBeNull();

		const start = await startFrom(s, en.mayTitle);
		expect(start.team.policy).toMatchObject({
			permissions: { run_commands: false, push: false },
		});
	});

	it("sets_or_clears_the_daily_limit", async () => {
		const first = await renderApp("/setup/spending");
		const s = first.socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await screen.findByRole("heading", { name: en.spendTitle });
		expect(
			(screen.getByRole("radio", { name: /^No limit/ }) as HTMLInputElement)
				.checked,
		).toBe(true);
		expect(screen.getByText(en.firstDay)).toBeTruthy();
		// The first-day figure is an estimate, for the six setup suggests.
		expect(en.firstDay).toMatch(/^We estimate .* suggested team of six /);
		fireEvent.click(screen.getByRole("radio", { name: /^Stop the team/ }));
		const amount = screen.getByLabelText(en.spendAmount) as HTMLInputElement;
		expect(amount.value).toBe("10");
		await expectNoAxeViolations(first.container);
		fireEvent.change(amount, { target: { value: "0" } });
		expect(screen.getByText(en.spendAmountWrong)).toBeTruthy();
		expect(
			(screen.getByRole("button", { name: en.continue }) as HTMLButtonElement)
				.disabled,
		).toBe(true);
		fireEvent.change(amount, { target: { value: "25" } });
		const set = await startFrom(s, en.spendTitle);
		expect(set.team.budgets).toEqual({ daily_usd: 25 });
		cleanup();

		const second = await renderApp("/setup/spending");
		const again = second.socket as FakeSocket;
		await answerQuery(
			again,
			"team.propose",
			proposed((team) => {
				team.budgets = { daily_usd: 25 };
			}),
		);
		await screen.findByRole("heading", { name: en.spendTitle });
		expect(
			(screen.getByLabelText(en.spendAmount) as HTMLInputElement).value,
		).toBe("25");
		fireEvent.click(screen.getByRole("radio", { name: /^No limit/ }));
		const cleared = await startFrom(again, en.spendTitle);
		expect(cleared.team.budgets).toEqual({});
	});

	it("chooses_how_work_is_finished", async () => {
		const { container, socket } = await renderApp("/setup/finish");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await screen.findByRole("heading", { name: en.finishTitle });
		expect(
			(
				screen.getByRole("radio", {
					name: /^Farik adds it/,
				}) as HTMLInputElement
			).checked,
		).toBe(true);
		await expectNoAxeViolations(container);
		fireEvent.click(
			screen.getByRole("radio", { name: /^Open a pull request/ }),
		);

		// Advanced and back keeps the choice.
		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		fireEvent.click(
			await screen.findByRole("button", { name: en.advancedHide }),
		);
		expect(
			(
				(await screen.findByRole("radio", {
					name: /^Open a pull request/,
				})) as HTMLInputElement
			).checked,
		).toBe(true);
		const start = await startFrom(s, en.finishTitle);
		expect(start.team.policy).toMatchObject({ integration: "pull_request" });
	});

	it("edits_the_checks_and_the_plan_check", async () => {
		const { container, socket } = await renderApp("/setup/advanced");
		const s = socket as FakeSocket;
		await answerQuery(
			s,
			"team.propose",
			proposed((team) => {
				team.agents = FIVE.filter((a) => a.role !== "architect");
			}),
		);
		// Who checks plans is the daemon's answer, not the page's own reckoning.
		const mira = {
			agent_id: "mira",
			display_name: "Mira",
			role: "product_manager",
		};
		await answerQuery(s, "team.validate", {
			errors: [],
			effects: [],
			agents: [],
			judges: { auto: mira, architect: null, scrum_master: null },
		});
		expect(
			await screen.findByRole("radio", {
				name: "Farik chooses: Mira, the Product Manager",
			}),
		).toBeTruthy();
		expect(
			screen.getByRole("radio", {
				name: "The Scrum Master, once the team has one",
			}),
		).toBeTruthy();
		const checks = await screen.findByRole("list", { name: en.checksTitle });
		expect(within(checks).getByText(TESTS_PASS.text)).toBeTruthy();
		expect(
			within(checks).getByText("Found in your project. Runs: pnpm test"),
		).toBeTruthy();
		expect(within(checks).getByText(en.checkReviewed)).toBeTruthy();
		await expectNoAxeViolations(container);

		const text = "The page loads in under two seconds.";
		fireEvent.change(screen.getByLabelText(en.checkNew), {
			target: { value: text },
		});
		fireEvent.click(screen.getByRole("button", { name: en.checkAdd }));
		const save = await sent(s, "criteria.save");
		const added = {
			name: "the-page-loads-in-under-two-seconds",
			text,
			source: "human",
			verification: { method: "review", rubric: [text] },
		};
		expect(save.params).toEqual({
			criteria: { criteria: [TESTS_PASS, added] },
		});
		await s.reply(save, {});
		expect(await within(checks).findByText(text)).toBeTruthy();

		const small = screen.getByRole("checkbox", {
			name: new RegExp(`^${en.planSmall}`),
		});
		expect((small as HTMLInputElement).checked).toBe(false);
		fireEvent.click(small);
		expect((small as HTMLInputElement).checked).toBe(true);

		fireEvent.click(screen.getByRole("radio", { name: /the Architect/i }));
		await waitFor(async () => {
			const frame = await asked(s, "team.validate");
			const team = (
				frame.params.params as {
					team: { policy: { judgment: { judge: string } } };
				}
			).team;
			expect(team.policy.judgment.judge).toBe("architect");
		});
		const refusal = "No active Architect can check plans.";
		await s.reply(await asked(s, "team.validate"), {
			errors: [
				{
					path: "/policy/judgment/judge",
					message: refusal,
					code: "judge_not_held",
				},
			],
			effects: [],
		});
		// Said at the choice it concerns, in the page's own words.
		const who = screen.getByRole("group", { name: en.planJudge });
		expect(
			await within(who).findByText(
				en.refuseJudge.replaceAll("{role}", en.roleArchitect),
			),
		).toBeTruthy();
		expect(who.getAttribute("aria-describedby")).toBeTruthy();
		expect(screen.queryByText(refusal)).toBeNull();
		expect(
			(screen.getByRole("button", { name: en.startTeam }) as HTMLButtonElement)
				.disabled,
		).toBe(true);

		fireEvent.click(screen.getByRole("radio", { name: /^Farik chooses/ }));
		await answerQuery(s, "team.validate", { errors: [], effects: [] });
		await waitFor(() =>
			expect(
				screen.queryByText(
					en.refuseJudge.replaceAll("{role}", en.roleArchitect),
				),
			).toBeNull(),
		);
		fireEvent.click(screen.getByRole("button", { name: en.startTeam }));
		const start = (await sent(s, "team.start")).params as {
			team: { policy: { judgment: unknown } };
			criteria: unknown;
		};
		expect(start.team.policy.judgment).toEqual({
			required: "always",
			questions: [BUDGET, NOTICE, SMALL],
			judge: "auto",
		});
		expect(start.criteria).toEqual({ criteria: [TESTS_PASS, added] });
	});

	it("says_a_refused_check_or_note_in_plain_words", async () => {
		const RAW = "/criteria/1/name does not match ^[a-z0-9-]+$";
		const { socket } = await renderApp("/setup/advanced");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		fireEvent.change(await screen.findByLabelText(en.checkNew), {
			target: { value: "The page loads in under two seconds." },
		});
		fireEvent.click(screen.getByRole("button", { name: en.checkAdd }));
		await s.fail(await sent(s, "criteria.save"), -32602, RAW);
		expect(await screen.findByText(en.refuseOther)).toBeTruthy();
		expect(screen.queryByText(/does not match/)).toBeNull();
		cleanup();

		const scan = await renderApp("/setup/scan");
		const n = scan.socket as FakeSocket;
		fireEvent.click(await screen.findByRole("button", { name: en.scanWrong }));
		fireEvent.change(screen.getByLabelText(en.scanWrongField), {
			target: { value: "It is a shop, not a game." },
		});
		fireEvent.click(screen.getByRole("button", { name: en.scanWrongSave }));
		const note = await sent(n, "project.note");
		await n.fail(note, -32603, "io error: permission denied (os error 13)");
		expect(await screen.findByText(en.refuseOther)).toBeTruthy();
		expect(screen.queryByText(/os error/)).toBeNull();
	});

	it("puts_each_setting_back_to_its_default", async () => {
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
		const changed = proposed((team) => {
			team.budgets = { daily_usd: 25 };
			team.rules = { require_new_tests: true };
			const policy = team.policy as Record<string, unknown>;
			policy.integration = "manual";
			policy.judgment = {
				required: "never",
				questions: [SMALL],
				judge: "auto",
			};
		});
		const { socket } = await renderApp("/setup/permissions");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", changed);
		/** Answers the defaults once the screen asked for them `n` times in all. */
		const answerDefaults = async (n: number) => {
			const asked = () =>
				s.calls("query").filter((f) => f.params.name === "settings.defaults");
			await waitFor(() => expect(asked().length).toBeGreaterThanOrEqual(n));
			for (const frame of asked()) await s.reply(frame, DEFAULTS);
			await waitFor(() =>
				expect(
					screen
						.getAllByRole("button", { name: en.putBack })
						.every((b) => !(b as HTMLButtonElement).disabled),
				).toBe(true),
			);
		};
		await answerDefaults(1);
		const back = () => screen.getByRole("button", { name: en.putBack });

		await screen.findByRole("heading", { name: en.mayTitle });
		fireEvent.click(back());
		expect(
			(
				screen.getByRole("radio", {
					name: /^Yes, the Developer/,
				}) as HTMLInputElement
			).checked,
		).toBe(true);
		expect(
			(
				screen.getByRole("radio", {
					name: /^No, keep everything/,
				}) as HTMLInputElement
			).checked,
		).toBe(true);
		fireEvent.click(screen.getByRole("button", { name: en.continue }));

		await screen.findByRole("heading", { name: en.spendTitle });
		await answerDefaults(2);
		fireEvent.click(back());
		expect(
			(screen.getByRole("radio", { name: /^No limit/ }) as HTMLInputElement)
				.checked,
		).toBe(true);
		fireEvent.click(screen.getByRole("button", { name: en.continue }));

		await screen.findByRole("heading", { name: en.finishTitle });
		await answerDefaults(3);
		fireEvent.click(back());
		expect(
			(
				screen.getByRole("radio", {
					name: /^Farik adds it/,
				}) as HTMLInputElement
			).checked,
		).toBe(true);

		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		await screen.findByRole("button", { name: en.advancedHide });
		await answerDefaults(4);
		const backs = screen.getAllByRole("button", { name: en.putBack });
		expect(backs).toHaveLength(2);
		for (const one of backs) fireEvent.click(one);
		fireEvent.click(screen.getByRole("button", { name: en.startTeam }));
		const start = (await sent(s, "team.start")).params as {
			team: {
				budgets: unknown;
				rules: unknown;
				policy: Record<string, unknown>;
			};
		};
		expect(start.team.budgets).toEqual({});
		expect(start.team.rules).toEqual({});
		expect(start.team.policy).toMatchObject(DEFAULTS.policy);
	});

	it("starts_the_team", async () => {
		const { socket } = await renderApp("/setup/finish");
		const s = socket as FakeSocket;
		const wire = proposed();
		await answerQuery(s, "team.propose", wire);
		fireEvent.click(await screen.findByRole("button", { name: en.startTeam }));
		const refused = await sent(s, "team.start");
		expect(refused.params).toEqual(wire);
		await s.fail(refused, -32005, "the team could not start");
		expect((await screen.findByRole("alert")).textContent).toContain(
			en.refuseOther,
		);
		expect(screen.queryByText(/could not start/)).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: en.startTeam }));
		await waitFor(() => expect(s.calls("team.start")).toHaveLength(2));
		await s.reply(s.calls("team.start")[1] as never, {});
		await answerStatus(s, false, 1, { setup_pending: false });
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
	});
});
