import { toCamel } from "@catervas/protocol-client";
import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { en } from "../../strings/en.ts";
import { t } from "../../strings/t.ts";
import type { FakeSocket } from "../../test/fake-socket.ts";
import {
	answerQuery,
	answerStatus,
	renderApp,
} from "../../test/render-app.tsx";
import { onlyInTips } from "../../test/tips.ts";
import type { Template } from "../SavedTeams.tsx";
import { type Agent, draftOf, type Proposed, someone } from "./TeamSetup.tsx";

/** What `settings.defaults` answers for the permissions: run commands, do not push. */
const RECOMMENDED = {
	policy: { permissions: { run_commands: true, push: false } },
	budgets: {},
	rules: {},
};
const BUDGET = "Does the task fit its budget?";
const NOTICE =
	"Would its checks notice if the work went wrong the way its intent worries about?";

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	persona: `${name} persona`,
	status: "active",
	model: { id: "claude-opus-5-5", effort: "high" },
});
const FIVE = [
	agent("mira", "Mira", "product_manager", "product-manager"),
	agent("sol", "Sol", "scrum_master", "scrum-master"),
	agent("ada", "Ada", "architect", "architect"),
	agent("theo", "Theo", "software_developer", "developer"),
	agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
];
const IRIS = agent("iris", "Iris", "ui_ux_designer", "extra-1");
/** The six Catervas suggests: the five, with the Designer after the Developer. */
const SIX = [...FIVE.slice(0, 4), IRIS, ...FIVE.slice(4)];
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
	afterEach(() => sessionStorage.clear());

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
		await answerStatus(t2, false, 1, { setup_pending: true });
		expect(
			await screen.findByRole("heading", { name: en.teamTitle }),
		).toBeTruthy();
	});

	it("someone_gives_finance_its_picture", () => {
		const like = (role: Agent["role"]): Agent => ({
			id: "",
			displayName: "",
			role,
			status: "active",
		});
		const team = FIVE.map(
			(a): Agent => ({
				id: a.id,
				displayName: a.display_name,
				role: a.role as Agent["role"],
				avatar: a.avatar,
				status: "active",
			}),
		);
		const finance = someone(team, like("finance_specialist"));
		expect(finance.avatar).toBe("finance-specialist");
		expect(finance.displayName).toBe("Noor");
		// Another Finance Specialist still takes its own picture, and the next spare name.
		const second = someone([...team, finance], like("finance_specialist"));
		expect(second.avatar).toBe("finance-specialist");
		expect(second.displayName).toBe("Ivo");
		// Every other role keeps the extras, never Iris's, the Finance Specialist's or the
		// Procurement Specialist's.
		const developer = someone([...team, finance], like("software_developer"));
		expect(["extra-2", "extra-3"]).toContain(developer.avatar);
	});

	it("someone_gives_procurement_its_picture", () => {
		const like = (role: Agent["role"]): Agent => ({
			id: "",
			displayName: "",
			role,
			status: "active",
		});
		const team = FIVE.map(
			(a): Agent => ({
				id: a.id,
				displayName: a.display_name,
				role: a.role as Agent["role"],
				avatar: a.avatar,
				status: "active",
			}),
		);
		const procurement = someone(team, like("procurement_specialist"));
		expect(procurement.avatar).toBe("extra-5");
		expect(procurement.displayName).toBe("Noor");
		// Another takes the same picture, and the next spare name.
		const second = someone(
			[...team, procurement],
			like("procurement_specialist"),
		);
		expect(second.avatar).toBe("extra-5");
		expect(second.displayName).toBe("Ivo");
	});

	it("someone_never_draws_the_fifth_extra_for_another_role", () => {
		// With no Procurement Specialist among the agents, nothing else holds `extra-5` for it, so
		// the pictures an added agent draws from must not include it. The team is the six, so that
		// the third agent's picture is the fallback's first, as `added_agents_draw_from_two_extras`
		// holds through the page.
		const developer: Agent = {
			id: "",
			displayName: "",
			role: "software_developer",
			status: "active",
		};
		let team = SIX.map(
			(a): Agent => ({
				id: a.id,
				displayName: a.display_name,
				role: a.role as Agent["role"],
				avatar: a.avatar,
				status: "active",
			}),
		);
		expect(team.some((a) => a.role === "procurement_specialist")).toBe(false);
		const pictures: (string | undefined)[] = [];
		for (let added = 0; added < 3; added++) {
			const next = someone(team, developer);
			pictures.push(next.avatar);
			team = [...team, next];
		}
		expect(pictures).toEqual(["extra-2", "extra-3", "extra-2"]);
	});

	it("shows_what_each_agent_may_do_on_request", async () => {
		const { socket } = await renderApp("/setup/permissions");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.mayTitle });
		expect(screen.queryByRole("heading", { name: en.mayAlready })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: en.mayAlreadyShow }));
		expect(
			screen
				.getByRole("button", { name: en.mayAlreadyHide })
				.getAttribute("aria-expanded"),
		).toBe("true");
		expect(screen.getByRole("heading", { name: en.mayAlready })).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: en.mayAlreadyHide }));
		expect(screen.queryByRole("heading", { name: en.mayAlready })).toBeNull();
	});

	it("keeps_an_answer_given_before_the_defaults", async () => {
		const { socket } = await renderApp("/setup/permissions");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.mayTitle });
		const radio = (name: RegExp) =>
			screen.getByRole("radio", { name }) as HTMLInputElement;
		fireEvent.click(radio(/^No, nobody may/));
		await answerQuery(s, "settings.defaults", RECOMMENDED);
		await waitFor(() =>
			expect(radio(/^No, keep work in progress on this computer/).checked).toBe(
				true,
			),
		);
		expect(radio(/^No, nobody may/).checked).toBe(true);
	});

	it("asks_both_permission_questions", async () => {
		const { container, socket } = await renderApp("/setup/permissions");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.mayTitle });
		const onward = () =>
			screen.getByRole("button", { name: en.continue }) as HTMLButtonElement;
		// The questions start at the recommended answers, once the defaults are known.
		await answerQuery(s, "settings.defaults", RECOMMENDED);
		await waitFor(() =>
			expect(
				(
					screen.getByRole("radio", {
						name: /^Yes, the Developer and Architect may/,
					}) as HTMLInputElement
				).checked,
			).toBe(true),
		);
		expect(
			(
				screen.getByRole("radio", {
					name: /^No, keep work in progress on this computer/,
				}) as HTMLInputElement
			).checked,
		).toBe(true);
		expect(onward().disabled).toBe(false);
		expect(screen.queryByText(/Nothing runs until/)).toBeNull();
		expect(screen.queryByText(/Answer both questions/)).toBeNull();
		for (const line of en.mayCommandsNote.split("\n")) onlyInTips(line);
		for (const line of t("mayPushNoteOne", { name: "Theo" }).split("\n"))
			onlyInTips(line);
		// What each agent may do is the daemon's answer for the team as it stands.
		const theo = (tiers: string[]) => ({
			errors: [],
			effects: [],
			agents: [{ id: "theo", tiers }],
		});
		fireEvent.click(screen.getByRole("button", { name: en.mayAlreadyShow }));
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
		expect(onward().disabled).toBe(false);

		const start = await startFrom(s, en.mayTitle);
		expect(start.team.policy).toMatchObject({
			permissions: { run_commands: false, push: false },
		});
	});

	it("sets_or_clears_the_daily_limit", async () => {
		const first = await renderApp("/setup/spending");
		const s = first.socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.spendTitle });
		expect(
			(screen.getByRole("radio", { name: /^No limit/ }) as HTMLInputElement)
				.checked,
		).toBe(true);
		expect(screen.getByText(en.firstDay)).toBeTruthy();
		// The first-day figure is an estimate, for the six setup suggests.
		// A scaled figure, not a measured one, so it is "about", never a bound.
		expect(en.firstDay).toMatch(
			/suggested team of six costs about twenty-five dollars \(an estimate\)/,
		);
		expect(en.firstDay).not.toMatch(/under/);
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
		await answerStatus(again, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.spendTitle });
		expect(
			(screen.getByLabelText(en.spendAmount) as HTMLInputElement).value,
		).toBe("25");
		fireEvent.click(screen.getByRole("radio", { name: /^No limit/ }));
		const cleared = await startFrom(again, en.spendTitle);
		expect(cleared.team.budgets).toEqual({});
	});

	it("keeps_the_team_answers_when_going_back", async () => {
		const first = await renderApp("/setup/team");
		const s = first.socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await answerQuery(s, "templates.list", NO_FOLDER);
		await answerQuery(s, "models.list", MODEL_LIST);
		fireEvent.click(await screen.findByRole("button", { name: "Change Theo" }));
		fireEvent.change(
			await screen.findByRole("textbox", { name: en.sheetName }),
			{
				target: { value: "Theodora" },
			},
		);
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		fireEvent.click(
			within(screen.getByRole("navigation", { name: en.setupSteps })).getByRole(
				"button",
				{ name: en.wizardFound },
			),
		);
		await screen.findByRole("heading", { name: en.scanTitle });
		fireEvent.click(screen.getByRole("button", { name: en.scanRight }));
		await answerQuery(s, "team.propose", proposed());
		expect(screen.queryByRole("heading", { name: en.teamTitle })).toBeNull();
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.teamTitle });
		await answerQuery(s, "templates.list", NO_FOLDER);
		expect(
			await screen.findByRole("button", { name: "Change Theodora" }),
		).toBeTruthy();
		cleanup();

		// Another project starts from the suggested names again.
		const second = await renderApp("/setup/team");
		await answerQuery(second.socket as FakeSocket, "team.propose", proposed());
		await answerStatus(second.socket as FakeSocket, false, 1, {
			project_root: "/home/me/other",
			setup_pending: true,
		});
		await screen.findByRole("heading", { name: en.teamTitle });
		expect(
			await screen.findByRole("button", { name: "Change Theo" }),
		).toBeTruthy();
		expect(
			screen.queryByRole("button", { name: "Change Theodora" }),
		).toBeNull();
	});

	it("spending_back_keeps_the_typed_limit", async () => {
		const { socket } = await renderApp("/setup/spending");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.spendTitle });
		fireEvent.click(screen.getByRole("radio", { name: /^Stop the team/ }));
		fireEvent.change(screen.getByLabelText(en.spendAmount), {
			target: { value: "25" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.back }));
		await screen.findByRole("heading", { name: en.mayTitle });
		await answerQuery(s, "settings.defaults", RECOMMENDED);
		await waitFor(() =>
			expect(
				(screen.getByRole("button", { name: en.continue }) as HTMLButtonElement)
					.disabled,
			).toBe(false),
		);
		fireEvent.click(screen.getByRole("button", { name: en.continue }));
		await screen.findByRole("heading", { name: en.spendTitle });
		expect(
			(screen.getByLabelText(en.spendAmount) as HTMLInputElement).value,
		).toBe("25");

		// A limit left unfinished is not kept: going back and on leaves the draft as it was.
		const around = async () => {
			fireEvent.click(screen.getByRole("button", { name: en.back }));
			await screen.findByRole("heading", { name: en.mayTitle });
			await answerQuery(s, "settings.defaults", RECOMMENDED);
			await waitFor(() =>
				expect(
					(
						screen.getByRole("button", {
							name: en.continue,
						}) as HTMLButtonElement
					).disabled,
				).toBe(false),
			);
			fireEvent.click(screen.getByRole("button", { name: en.continue }));
			await screen.findByRole("heading", { name: en.spendTitle });
		};
		fireEvent.click(screen.getByRole("radio", { name: /^No limit/ }));
		await around();
		fireEvent.click(screen.getByRole("radio", { name: /^Stop the team/ }));
		fireEvent.change(screen.getByLabelText(en.spendAmount), {
			target: { value: "-5" },
		});
		await around();
		expect(
			(screen.getByRole("radio", { name: /^No limit/ }) as HTMLInputElement)
				.checked,
		).toBe(true);
	});

	it("spending_step_link_keeps_the_typed_limit", async () => {
		const { socket } = await renderApp("/setup/spending");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.spendTitle });
		fireEvent.click(screen.getByRole("radio", { name: /^Stop the team/ }));
		fireEvent.change(screen.getByLabelText(en.spendAmount), {
			target: { value: "25" },
		});
		fireEvent.click(
			within(screen.getByRole("navigation", { name: en.setupSteps })).getByRole(
				"button",
				{ name: en.wizardMay },
			),
		);
		await screen.findByRole("heading", { name: en.mayTitle });
		await answerQuery(s, "settings.defaults", RECOMMENDED);
		await waitFor(() =>
			expect(
				(screen.getByRole("button", { name: en.continue }) as HTMLButtonElement)
					.disabled,
			).toBe(false),
		);
		fireEvent.click(screen.getByRole("button", { name: en.continue }));
		await screen.findByRole("heading", { name: en.spendTitle });
		expect(
			(screen.getByLabelText(en.spendAmount) as HTMLInputElement).value,
		).toBe("25");
	});

	it("ignores_a_kept_draft_of_another_shape", async () => {
		sessionStorage.setItem(
			"catervas.setupDraft",
			JSON.stringify({
				root: "/home/me/corner-bakery",
				draft: { team: {} },
			}),
		);
		const { socket } = await renderApp("/setup/team");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		expect(
			await screen.findByRole("button", { name: "Change Theo" }),
		).toBeTruthy();
	});

	it("forgets_the_draft_once_the_team_starts", async () => {
		const { socket } = await renderApp("/setup/finish");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.finishTitle });
		fireEvent.click(screen.getByRole("button", { name: en.advancedSwitch }));
		fireEvent.click(await screen.findByRole("switch", { name: en.ruleTests }));
		expect(sessionStorage.getItem("catervas.setupDraft")).not.toBeNull();
		fireEvent.click(screen.getByRole("button", { name: en.startTeam }));
		await s.reply(await sent(s, "team.start"), {});
		await waitFor(() =>
			expect(sessionStorage.getItem("catervas.setupDraft")).toBeNull(),
		);
	});

	it("starts_with_the_documents_kept_on_this_computer", async () => {
		const { socket } = await renderApp("/setup/finish");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.finishTitle });
		fireEvent.click(screen.getByRole("button", { name: en.advancedSwitch }));
		const keep = await screen.findByRole("switch", { name: en.documentsLocal });
		expect(keep.getAttribute("aria-checked")).toBe("false");
		fireEvent.click(keep);
		expect(keep.getAttribute("aria-checked")).toBe("true");
		fireEvent.click(screen.getByRole("button", { name: en.startTeam }));
		const frame = await sent(s, "team.start");
		expect(
			(frame.params as { team: { policy: Record<string, unknown> } }).team
				.policy,
		).toMatchObject({ documents_local: true });
	});

	it("shows_an_example_of_the_limit", async () => {
		const { socket } = await renderApp("/setup/spending");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.spendTitle });
		expect(screen.queryByText(en.spendExample)).toBeNull();
		fireEvent.click(screen.getByRole("radio", { name: /^Stop the team/ }));
		fireEvent.change(screen.getByLabelText(en.spendAmount), {
			target: { value: "20" },
		});
		expect(
			screen.getByText(
				"Say the team spends $12.80 in a day: $7.20 of $20.00 is left.",
			),
		).toBeTruthy();
		expect(screen.queryByText(/Today the team has spent/)).toBeNull();
		expect(
			screen.getByText("At $20.00, everyone pauses and you get a message."),
		).toBeTruthy();
		const figure = screen
			.getByText(en.spendExample)
			.closest("figure") as HTMLElement;
		expect(
			screen.getByLabelText(en.spendAmount).compareDocumentPosition(figure) &
				Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		const meter = screen.getByRole("meter", {
			name: "Say the team spends $12.80 in a day: $7.20 of $20.00 is left.",
		}) as HTMLMeterElement;
		expect(meter.value).toBeCloseTo(12.8);
		expect(meter.max).toBe(20);
		fireEvent.change(screen.getByLabelText(en.spendAmount), {
			target: { value: "0" },
		});
		expect(screen.queryByText(en.spendExample)).toBeNull();
		fireEvent.click(screen.getByRole("radio", { name: /^No limit/ }));
		expect(screen.queryByText(en.spendExample)).toBeNull();
	});

	it("opens_advanced_settings_in_place_on_finish", async () => {
		const { socket } = await renderApp("/setup/finish");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.finishTitle });
		expect(
			screen.queryByRole("switch", { name: en.advancedSwitch }),
		).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: en.advancedSwitch }));
		expect(
			screen
				.getByRole("button", { name: en.advancedHide })
				.getAttribute("aria-expanded"),
		).toBe("true");
		expect(
			screen
				.getByRole("button", { name: en.advancedHide })
				.getAttribute("aria-controls"),
		).toBe("finish-advanced");
		expect(screen.getByRole("heading", { name: en.finishTitle })).toBeTruthy();
		expect(await screen.findByText(en.rulesTitle)).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: en.advancedHide }));
		expect(screen.queryByText(en.rulesTitle)).toBeNull();
	});

	it("the_old_advanced_address_goes_to_finish", async () => {
		const { socket } = await renderApp("/setup/advanced");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		expect(
			await screen.findByRole("heading", { name: en.finishTitle }),
		).toBeTruthy();
	});

	it("setup_and_rules_offer_no_integration_choice", async () => {
		// Finished work always opens a pull request (ADR 0061); merging it is the switch's (ADR 0073).
		const first = await renderApp("/setup/finish");
		const s = first.socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.finishTitle });
		expect(screen.queryByRole("group", { name: "Accepted work" })).toBeNull();
		expect(screen.queryByRole("radio", { name: /pull request/i })).toBeNull();
		expect(screen.queryByRole("button", { name: en.putBack })).toBeNull();
		cleanup();

		const rules = await renderApp("/settings/work");
		const r = rules.socket as FakeSocket;
		await answerStatus(r, false);
		await answerQuery(r, "team.get", {
			team: { ...proposed().team, rules: {} },
			agents: [],
			max_agents: 7,
			online: true,
		});
		await answerQuery(r, "settings.defaults", RECOMMENDED);
		const finish = await screen.findByRole("region", {
			name: en.rulesFinish,
		});
		expect(within(finish).getByText(en.finishLead)).toBeTruthy();
		expect(within(finish).queryByRole("radio")).toBeNull();
		expect(
			within(finish).queryByRole("button", { name: en.putBack }),
		).toBeNull();
		// The one control is the switch that lets Catervas merge (ADR 0073), off until confirmed.
		expect(
			within(finish)
				.getByRole("switch", { name: en.mergingSwitch })
				.getAttribute("aria-checked"),
		).toBe("false");
	});

	it("edits_the_checks", async () => {
		const { container, socket } = await renderApp("/setup/finish");
		const advanced = () =>
			within(document.getElementById("finish-advanced") as HTMLElement);
		const s = socket as FakeSocket;
		await answerQuery(
			s,
			"team.propose",
			proposed((team) => {
				team.agents = FIVE.filter((a) => a.role !== "architect");
			}),
		);
		await answerStatus(s, false, 1, { setup_pending: true });
		fireEvent.click(
			await screen.findByRole("button", { name: en.advancedSwitch }),
		);
		await answerQuery(s, "team.validate", {
			errors: [],
			effects: [],
			agents: [],
		});
		const checks = await advanced().findByRole("list", {
			name: en.checksTitle,
		});
		expect(within(checks).getByText(TESTS_PASS.text)).toBeTruthy();
		expect(
			within(checks).getByText("Found in your project. Runs: pnpm test"),
		).toBeTruthy();
		expect(within(checks).getByText(en.checkReviewed)).toBeTruthy();
		await expectNoAxeViolations(container);

		const text = "The page loads in under two seconds.";
		fireEvent.change(advanced().getByLabelText(en.checkNew), {
			target: { value: text },
		});
		fireEvent.click(advanced().getByRole("button", { name: en.checkAdd }));
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

		fireEvent.click(screen.getByRole("button", { name: en.startTeam }));
		const start = (await sent(s, "team.start")).params as {
			criteria: unknown;
		};
		expect(start.criteria).toEqual({ criteria: [TESTS_PASS, added] });
	});

	it("setup_and_rules_ask_nothing_about_checking_plans", async () => {
		// ADR 0061: no agent checks a plan. A team file may still carry `policy.judgment`
		// (proposed() does), and both pages open on it with nothing said about checking.
		const none = (where: HTMLElement) => {
			expect(screen.queryByRole("group", { name: /^Who checks/ })).toBeNull();
			expect(
				screen.queryByRole("switch", { name: /Check every plan/ }),
			).toBeNull();
			expect(
				screen.queryByRole("checkbox", { name: /within its budget/ }),
			).toBeNull();
			expect(
				screen.queryByRole("heading", { name: /^Checking plans/ }),
			).toBeNull();
			expect(where.textContent ?? "").not.toMatch(/Who checks/);
		};
		const { socket } = await renderApp("/setup/finish");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		fireEvent.click(
			await screen.findByRole("button", { name: en.advancedSwitch }),
		);
		await answerQuery(s, "team.validate", {
			errors: [],
			effects: [],
			agents: [],
		});
		await screen.findByRole("list", { name: en.checksTitle });
		none(document.getElementById("finish-advanced") as HTMLElement);
		cleanup();

		const rules = await renderApp("/settings/work");
		const r = rules.socket as FakeSocket;
		await answerStatus(r, false);
		await answerQuery(r, "team.get", {
			team: proposed().team,
			agents: [],
			max_agents: 7,
		});
		await answerQuery(r, "settings.defaults", RECOMMENDED);
		await screen.findByRole("region", { name: en.rulesFinish });
		none(screen.getByRole("main"));
		expect(screen.queryByRole("alert")).toBeNull();
	});

	it("says_a_refused_check_or_note_in_plain_words", async () => {
		const RAW = "/criteria/1/name does not match ^[a-z0-9-]+$";
		const { socket } = await renderApp("/setup/finish");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", proposed());
		await answerStatus(s, false, 1, { setup_pending: true });
		fireEvent.click(
			await screen.findByRole("button", { name: en.advancedSwitch }),
		);
		const advanced = within(
			document.getElementById("finish-advanced") as HTMLElement,
		);
		fireEvent.change(await advanced.findByLabelText(en.checkNew), {
			target: { value: "The page loads in under two seconds." },
		});
		fireEvent.click(advanced.getByRole("button", { name: en.checkAdd }));
		await s.fail(await sent(s, "criteria.save"), -32602, RAW);
		expect(await screen.findByText(en.refuseOther)).toBeTruthy();
		expect(screen.queryByText(/does not match/)).toBeNull();

		// Rules typed as text that do not parse are said plainly, not by the parser.
		fireEvent.click(screen.getByRole("button", { name: en.editAsText }));
		fireEvent.change(screen.getByLabelText(en.rulesText), {
			target: { value: "{ not json" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.rulesTextUse }));
		expect(await screen.findByText(en.rulesTextWrong)).toBeTruthy();
		expect(screen.queryByText(/JSON|position|token/)).toBeNull();
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
				permissions: { run_commands: true, push: false },
			},
		};
		const changed = proposed((team) => {
			team.budgets = { daily_usd: 25 };
			team.rules = { require_new_tests: true };
		});
		const { socket } = await renderApp("/setup/permissions");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", changed);
		await answerStatus(s, false, 1, { setup_pending: true });
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
					name: /^No, keep work in progress/,
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
		fireEvent.click(screen.getByRole("button", { name: en.advancedSwitch }));
		await screen.findByRole("button", { name: en.advancedHide });
		await answerDefaults(3);
		const backs = within(
			document.getElementById("finish-advanced") as HTMLElement,
		).getAllByRole("button", { name: en.putBack });
		expect(backs).toHaveLength(1);
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

	it("setup_and_settings_never_ask_about_sprints", async () => {
		const first = await renderApp("/setup/finish");
		const s = first.socket as FakeSocket;
		// team.propose still answers plan_in_sprints (plan 5 takes it off the wire).
		await answerQuery(
			s,
			"team.propose",
			proposed((team) => {
				(team.policy as Record<string, unknown>).plan_in_sprints = true;
			}),
		);
		await answerStatus(s, false, 1, { setup_pending: true });
		await screen.findByRole("heading", { name: en.finishTitle });
		fireEvent.click(screen.getByRole("button", { name: en.advancedSwitch }));
		await screen.findByRole("button", { name: en.checkAdd });
		expect(screen.queryByRole("switch", { name: /sprint/i })).toBeNull();
		expect(document.body.textContent).not.toMatch(/sprint/i);
		await expectNoAxeViolations(first.container);
		fireEvent.click(screen.getByRole("button", { name: en.startTeam }));
		const start = (await sent(s, "team.start")).params as {
			team: { policy: Record<string, unknown> };
		};
		expect("plan_in_sprints" in start.team.policy).toBe(false);
		cleanup();

		const second = await renderApp("/settings/work");
		const s2 = second.socket as FakeSocket;
		await answerStatus(s2, false);
		await answerQuery(s2, "team.get", {
			team: proposed().team,
			agents: [],
			max_agents: 7,
		});
		await answerQuery(s2, "settings.defaults", {
			...RECOMMENDED,
			policy: { ...RECOMMENDED.policy, integration: "auto_merge" },
		});
		const main = await screen.findByRole("main");
		await within(main).findByRole("region", { name: en.rulesDocuments });
		expect(within(main).queryByRole("switch", { name: /sprint/i })).toBeNull();
		expect(main.textContent).not.toMatch(/sprint/i);
	});

	it("keeps_a_saved_teams_documents_choice_and_drops_its_sprint_answer", () => {
		const wire = toCamel(proposed()) as Proposed;
		const template = toCamel(THREE.template) as Template;
		expect(
			draftOf(wire, "saved", {
				...template,
				policy: { ...template.policy, documentFormat: "markdown" },
			}).team.policy.documentFormat,
		).toBe("markdown");
	});

	it("starts_the_team", async () => {
		const { socket } = await renderApp("/setup/finish");
		const s = socket as FakeSocket;
		const wire = proposed();
		await answerQuery(s, "team.propose", wire);
		await answerStatus(s, false, 1, { setup_pending: true });
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
		// The offer to document the project has none for this team: it goes on to Today.
		await answerQuery(s, "catervafication.get", { offered: false, roles: [] });
		await answerStatus(s, false, 1, { setup_pending: false });
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
	});
});

const ready = (
	slug: string,
	name: string,
	agents: Record<string, string>[],
) => ({
	slug,
	template: {
		version: 1,
		name,
		saved_at: "2026-10-10T00:00:00Z",
		agents,
		policy: {
			permissions: { run_commands: true, push: false },
			integration: "manual",
		},
		budgets: {},
	},
});
const MIRA = {
	id: "mira",
	display_name: "Mira",
	role: "product_manager",
	avatar: "product-manager",
	persona: "Asks the questions that decide what to build",
};
const KAI = {
	id: "kai",
	display_name: "Kai",
	role: "marketing_specialist",
	avatar: "marketing-specialist",
	persona: "Owns your brand and how you reach people",
};
const NOOR_SHOP = {
	id: "noor",
	display_name: "Noor",
	role: "procurement_specialist",
	avatar: "extra-5",
	persona: "Finds the best seller at the right price",
};
const LENA_SHOP = {
	id: "lena",
	display_name: "Lena",
	role: "finance_specialist",
	avatar: "finance-specialist",
	persona: "Keeps your numbers straight",
};
const SOFTWARE = ready("software-product", "Software product", [
	MIRA,
	{
		id: "ada",
		display_name: "Ada",
		role: "architect",
		avatar: "architect",
		persona: "Thinks about how it all fits together",
	},
	{
		id: "theo",
		display_name: "Theo",
		role: "software_developer",
		avatar: "developer",
		persona: "Builds it and tests it",
	},
	KAI,
]);
const ONLINE = ready("online-store", "Online store", [
	MIRA,
	NOOR_SHOP,
	LENA_SHOP,
	KAI,
]);
const FINANCIAL = ready("financial-analysis", "Financial analysis", [
	MIRA,
	{ ...LENA_SHOP, id: "noor", display_name: "Noor", persona: "Portfolio desk" },
	{ ...LENA_SHOP, id: "ivo", display_name: "Ivo", persona: "Risk desk" },
	LENA_SHOP,
]);
const BUILTIN = [SOFTWARE, ONLINE, FINANCIAL];
/** A listing with no state folder: the ready-made teams alone. */
const NO_FOLDER = {
	folder: null,
	builtin: BUILTIN,
	templates: [],
	unreadable: [],
};

/** A saved team as `templates.list` answers it: Mira and Ada with the template's own personas, Theo with none. */
const THREE = {
	slug: "three-of-us",
	template: {
		version: 1,
		name: "Three of us",
		saved_at: "2026-09-28T12:00:00Z",
		agents: [
			{
				id: "mira",
				display_name: "Mira",
				role: "product_manager",
				persona: "Mira.",
				avatar: "product-manager",
				model: { id: "claude-sonnet-5", effort: "low" },
			},
			{ id: "ada", display_name: "Ada", role: "architect", persona: "Ada." },
			{ id: "theo", display_name: "Theo", role: "software_developer" },
		],
		policy: {
			permissions: { run_commands: true, push: false },
			integration: "pull_request",
		},
		budgets: { daily_usd: 20 },
	},
};
const LISTED = {
	folder: "/home/me/.config/catervas/templates",
	builtin: BUILTIN,
	unreadable: [],
};
/** What `models.list` answers: the newest of each family, by name. */
const MODEL_LIST = {
	models: [
		{ id: "claude-fable-5-1", label: "Fable 5.1" },
		{ id: "claude-opus-5-5", label: "Opus 5.5" },
		{ id: "claude-sonnet-5-5", label: "Sonnet 5.5" },
		{ id: "claude-haiku-5-5", label: "Haiku 5.5" },
	],
};
/** What `team.propose` says about the folder: the teams in the order it fits them. */
const CODE = {
	slug: "software-product",
	signal: "code",
	language: "TypeScript",
	tests: true,
};
const SHOP_FIT = {
	slug: "online-store",
	signal: "shop",
	language: null,
	tests: false,
};
const MONEY_FIT = {
	slug: "financial-analysis",
	signal: "money",
	language: null,
	tests: false,
};
const IRIS_WIRE = {
	id: "iris",
	display_name: "Iris",
	role: "ui_ux_designer",
	avatar: "extra-1",
};
/** The software team as Catervas ships it now: the Designer with the rest. */
const WITH_DESIGNER = ready("software-product", "Software product", [
	MIRA,
	{
		id: "theo",
		display_name: "Theo",
		role: "software_developer",
		avatar: "developer",
		persona: "Builds it and tests it",
	},
	IRIS_WIRE,
]);
const ROLE_DEFAULTS = {
	finance_specialist: {
		model: "claude-sonnet-5-5",
		effort: "low",
		persona: "Keeps the books",
	},
};

/** Opens setup's team step with this listing, folder fit and models answered. */
async function picker(
	order: unknown[],
	listing: unknown = NO_FOLDER,
	extra: Record<string, unknown> = {},
	team: (team: Record<string, unknown>) => void = () => {},
) {
	// Each opening is a first visit: no draft is kept from one before it in the test.
	sessionStorage.clear();
	const { container, socket } = await renderApp("/setup/team");
	const s = socket as FakeSocket;
	await answerQuery(s, "team.propose", {
		...proposed(team),
		unavailable: [],
		suggested_teams: order,
		role_defaults: ROLE_DEFAULTS,
		...extra,
	});
	await answerStatus(s, false, 1, { setup_pending: true });
	await answerQuery(s, "templates.list", listing);
	await answerQuery(s, "models.list", MODEL_LIST);
	return { container, s };
}
/** The ways to start, in the order the cards show them. */
const cardNames = () =>
	within(screen.getByRole("group", { name: en.pickStarts }))
		.getAllByRole("radio")
		.map((r) => (r.closest("label")?.textContent ?? "").replace(/^[✓+]/, ""));
/** The names on the team's rows. */
const rows = () =>
	screen
		.getAllByRole("button", { name: /^Change / })
		.map((b) => b.getAttribute("aria-label")?.replace("Change ", ""));
const sheet = () => within(screen.getByRole("dialog"));
const change = (name: string) =>
	fireEvent.click(screen.getByRole("button", { name: `Change ${name}` }));
/** The team the latest `team.validate` carried. */
const validatedTeam = async (s: FakeSocket) =>
	(
		(await asked(s, "team.validate")).params.params as {
			team: {
				agents: Record<string, unknown>[];
				policy: Record<string, unknown>;
				preview?: unknown;
			};
		}
	).team;
const noErrors = { errors: [], effects: [] };

describe("team setup's team picker", () => {
	afterEach(() => sessionStorage.clear());

	it("suggests_the_team_the_folder_fits_and_fills_who_is_on_it", async () => {
		const { container, s } = await picker([MONEY_FIT, CODE, SHOP_FIT], {
			...LISTED,
			templates: [THREE],
		});
		await screen.findByText(en.pickedMoney);
		// The folder's team first, big and marked; then the others, one agent, saved, your own.
		expect(
			await waitFor(() => {
				const names = cardNames();
				if (names.length < 6) throw new Error("the cards are not drawn yet");
				return names;
			}),
		).toEqual([
			expect.stringMatching(/^Financial analysis/),
			expect.stringMatching(/^Software product/),
			expect.stringMatching(/^Online store/),
			expect.stringMatching(new RegExp(`^${en.pickSolo}`)),
			expect.stringMatching(/^Three of us/),
			expect.stringMatching(new RegExp(`^${en.pickScratch}`)),
		]);
		const first = screen.getByRole("radio", { name: /^Financial analysis/ });
		expect((first as HTMLInputElement).checked).toBe(true);
		expect(screen.getAllByText(en.pickSuggested)).toHaveLength(1);
		expect(first.closest("label")?.textContent).toContain(en.pickSuggested);
		// A first visit fills the list from that first team.
		expect(rows()).toEqual(["Mira", "Noor", "Ivo", "Lena"]);
		expect(screen.getByRole("heading", { name: en.whoTitle })).toBeTruthy();
		await expectNoAxeViolations(container);

		// Choosing another card swaps the list; the saved one brings its own people and models.
		fireEvent.click(screen.getByRole("radio", { name: /^Three of us/ }));
		expect(rows()).toEqual(["Mira", "Ada", "Theo"]);
		const mira = screen.getByRole("button", { name: "Change Mira" });
		expect(mira.textContent).toContain("claude-sonnet-5");
		expect(mira.textContent).toContain("Low effort");
		fireEvent.click(screen.getByRole("radio", { name: /^Online store/ }));
		expect(rows()).toEqual(["Mira", "Noor", "Lena", "Kai"]);
		expect(
			(screen.getByRole("radio", { name: /^Online store/ }) as HTMLInputElement)
				.checked,
		).toBe(true);
		expect(s.calls("team.validate")).toHaveLength(0);
		cleanup();

		// Another folder, another reason: a shop's files, then code with tests.
		await picker([SHOP_FIT, CODE]);
		expect(await screen.findByText(en.pickedShop)).toBeTruthy();
		expect(
			(await screen.findByRole("radio", { name: /^Online store/ })).closest(
				"label",
			)?.textContent,
		).toContain(en.pickSuggested);
		cleanup();
		await picker([CODE]);
		expect(
			await screen.findByText(t("pickedCodeTests", { language: "TypeScript" })),
		).toBeTruthy();
	});

	it("opens_an_agent_sheet_with_its_model_effort_and_temperature", async () => {
		const { container, s } = await picker([CODE]);
		await screen.findByRole("button", { name: "Change Theo" });
		// A row names the model and effort; an agent with none of its own shows its role's.
		const theo = screen.getByRole("button", { name: "Change Theo" });
		expect(theo.textContent).toContain("Opus 5.5");
		expect(theo.textContent).toContain("High effort");
		change("Theo");
		const dialog = sheet();
		expect(
			(dialog.getByRole("textbox", { name: en.sheetName }) as HTMLInputElement)
				.value,
		).toBe("Theo");
		expect(dialog.getByRole("textbox", { name: en.sheetFocus })).toBeTruthy();
		// The job of a team's agent is fixed text.
		expect(dialog.queryByRole("combobox", { name: en.sheetJob })).toBeNull();
		const model = dialog.getByRole("group", { name: en.modelLegend });
		expect(
			within(model)
				.getAllByRole("radio")
				.map((r) => (r as HTMLInputElement).labels?.[0]?.textContent),
		).toEqual(["Fable 5.1", "Opus 5.5", "Sonnet 5.5", "Haiku 5.5"]);
		const effort = dialog.getByRole("group", { name: en.effortLegend });
		expect(
			within(effort)
				.getAllByRole("radio")
				.map((r) => (r as HTMLInputElement).labels?.[0]?.textContent),
		).toEqual(["Low", "Medium", "High", "Extra high", "Max"]);
		expect(dialog.getByText(en.temperatureLegend)).toBeTruthy();
		expect(dialog.getByText(en.temperatureValue)).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.change(dialog.getByRole("textbox", { name: en.sheetName }), {
			target: { value: "Theodora" },
		});
		fireEvent.click(within(model).getByRole("radio", { name: "Sonnet 5.5" }));
		fireEvent.click(within(effort).getByRole("radio", { name: "Extra high" }));
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		// The row shows the change at once, and the team carries it.
		const row = screen.getByRole("button", { name: "Change Theodora" });
		expect(row.textContent).toContain("Sonnet 5.5");
		expect(row.textContent).toContain("Extra high effort");
		// A role Catervas ships a default for shows it when the agent has none.
		fireEvent.click(screen.getByRole("radio", { name: /^Online store/ }));
		expect(
			screen.getByRole("button", { name: "Change Lena" }).textContent,
		).toContain("Low effort");
		fireEvent.click(screen.getByRole("radio", { name: /^Software product/ }));
		change("Theo");
		fireEvent.click(
			within(sheet().getByRole("group", { name: en.effortLegend })).getByRole(
				"radio",
				{ name: "Max" },
			),
		);
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		const team = await validatedTeam(s);
		expect(team.agents.find((a) => a.id === "theo")?.model).toEqual({
			id: "claude-opus-5-5",
			effort: "max",
		});
	});

	it("starts_one_agent_alone_and_drops_it_for_a_team", async () => {
		const { s } = await picker([CODE]);
		fireEvent.click(await screen.findByRole("radio", { name: /^One agent/ }));
		expect(
			await screen.findByRole("heading", { name: en.whoSoloTitle }),
		).toBeTruthy();
		expect(rows()).toEqual(["Theo"]);
		// No "Add someone" for one agent; Continue names it.
		expect(screen.queryByRole("button", { name: en.teamAdd })).toBeNull();
		expect(
			screen.getByRole("button", { name: t("soloContinue", { name: "Theo" }) }),
		).toBeTruthy();
		change("Theo");
		const job = sheet().getByRole("combobox", { name: en.sheetJob });
		const jobs = within(job)
			.getAllByRole("option")
			.map((o) => o.textContent);
		expect(jobs).toContain("Finance Specialist");
		expect(jobs).not.toContain("Scrum Master");
		expect(jobs).not.toContain("UI/UX Designer");
		expect(sheet().queryByRole("switch", { name: en.sheetOnTeam })).toBeNull();
		fireEvent.change(job, { target: { value: "finance_specialist" } });
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		fireEvent.click(
			screen.getByRole("button", { name: t("soloContinue", { name: "Noor" }) }),
		);
		const alone = await validatedTeam(s);
		expect(alone.agents).toHaveLength(1);
		expect(alone.agents[0]).toMatchObject({ role: "finance_specialist" });
		expect(alone.policy.solo).toBe(true);
		await s.reply(await asked(s, "team.validate"), {
			errors: [{ path: "/agents", message: "no", code: "too_many" }],
			effects: [],
		});
		await screen.findByRole("alert");

		// Picking a team again takes the flag off.
		fireEvent.click(screen.getByRole("radio", { name: /^Software product/ }));
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		await waitFor(() =>
			expect(
				s.calls("query").filter((f) => f.params.name === "team.validate"),
			).toHaveLength(2),
		);
		const team = await validatedTeam(s);
		expect(team.agents.length).toBeGreaterThan(1);
		expect("solo" in team.policy).toBe(false);
	});

	it("an_added_designer_starts_off_the_team_without_docker", async () => {
		const { s } = await picker(
			[CODE],
			NO_FOLDER,
			{ unavailable: [{ agent_id: "iris", reason: "designer_needs_sandbox" }] },
			(team) => {
				team.agents = [...FIVE.slice(0, 4), IRIS];
			},
		);
		await screen.findByRole("button", { name: "Change Theo" });
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		fireEvent.change(sheet().getByRole("combobox", { name: en.sheetJob }), {
			target: { value: "ui_ux_designer" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		const added = screen.getAllByRole("button", { name: /^Change / }).at(-1);
		expect(added?.textContent).toContain(en.whoNeedsDocker);
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		const team = await validatedTeam(s);
		expect(team.agents.map((a) => a.role)).not.toContain("ui_ux_designer");
	});

	it("holds_the_designer_off_without_docker_and_asks_how_to_open_the_app_with_it", async () => {
		const withDesigner =
			(preview?: unknown) => (team: Record<string, unknown>) => {
				team.agents = [...FIVE.slice(0, 4), IRIS];
				if (preview) team.preview = preview;
			};
		const listing = { ...NO_FOLDER, builtin: [WITH_DESIGNER] };
		// No Docker: the row says so, the sheet says how to get it, and the team goes without her.
		const no = await picker(
			[CODE],
			listing,
			{ unavailable: [{ agent_id: "iris", reason: "designer_needs_sandbox" }] },
			withDesigner(),
		);
		const iris = await screen.findByRole("button", { name: "Change Iris" });
		expect(iris.textContent).toContain(en.whoNeedsDocker);
		change("Iris");
		expect(sheet().getByText(en.teamNeedsSandbox)).toBeTruthy();
		expect(
			sheet().getByRole("button", { name: en.teamCheckAgain }),
		).toBeTruthy();
		expect(
			sheet().queryByRole("textbox", { name: en.previewStart }),
		).toBeNull();
		await expectNoAxeViolations(no.container);
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		const held = await validatedTeam(no.s);
		expect(held.agents.map((a) => a.id)).not.toContain("iris");
		expect("preview" in held).toBe(false);
		cleanup();

		// With Docker and a start command found: it is told, and "Change" opens the fields.
		const found = await picker(
			[CODE],
			listing,
			{},
			withDesigner({ start: "pnpm run dev", path: "/" }),
		);
		await screen.findByRole("button", { name: "Change Iris" });
		change("Iris");
		const card = sheet().getByRole("region", { name: en.previewTitle });
		expect(
			within(card).getByText(
				t("previewFound", { designer: "Iris", start: "pnpm run dev" }),
			),
		).toBeTruthy();
		expect(
			within(card).queryByRole("textbox", { name: en.previewStart }),
		).toBeNull();
		fireEvent.click(
			within(card).getByRole("button", { name: en.previewChange }),
		);
		const start = within(card).getByRole("textbox", {
			name: en.previewStart,
		}) as HTMLInputElement;
		await waitFor(() => expect(document.activeElement).toBe(start));
		expect(start.value).toBe("pnpm run dev");
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		// An empty port is left out.
		expect((await validatedTeam(found.s)).preview).toEqual({
			start: "pnpm run dev",
			path: "/",
		});
		cleanup();

		// Nothing found: the fields are shown, a refused port is told at the port, and taking
		// her off the team asks nothing and sends no preview.
		const none = await picker([CODE], listing, {}, withDesigner());
		await screen.findByRole("button", { name: "Change Iris" });
		change("Iris");
		const fields = sheet().getByRole("region", {
			name: "Iris needs to know how to open your app",
		});
		const field = (name: string) =>
			within(fields).getByRole("textbox", { name }) as HTMLInputElement;
		fireEvent.change(field(en.previewStart), { target: { value: "pnpm dev" } });
		fireEvent.change(field(en.previewPort), { target: { value: "80" } });
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		expect((await validatedTeam(none.s)).preview).toEqual({
			start: "pnpm dev",
			port: 80,
			path: "/",
		});
		await none.s.reply(await asked(none.s, "team.validate"), {
			errors: [
				{ path: "/preview/port", message: "80 is low", code: "invalid" },
			],
			effects: [],
		});
		change("Iris");
		expect(
			await within(sheet().getByRole("region")).findByText(
				en.refusePreviewPort,
			),
		).toBeTruthy();
		expect(
			(
				sheet()
					.getByRole("region")
					.querySelector(`[id$="-port"]`) as HTMLElement
			).getAttribute("aria-invalid"),
		).toBe("true");
		fireEvent.click(sheet().getByRole("switch", { name: en.sheetOnTeam }));
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		await waitFor(() =>
			expect(
				none.s.calls("query").filter((f) => f.params.name === "team.validate"),
			).toHaveLength(2),
		);
		const off = await validatedTeam(none.s);
		expect(off.agents.map((a) => a.id)).not.toContain("iris");
		expect("preview" in off).toBe(false);
	});

	it("adds_someone_with_a_spare_name_removes_them_and_stops_at_seven", async () => {
		const { s } = await picker([CODE]);
		await screen.findByRole("button", { name: "Change Theo" });
		fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
		// The new row's sheet opens at once, with a job to choose and a spare name.
		const dialog = sheet();
		expect(
			(dialog.getByRole("textbox", { name: en.sheetName }) as HTMLInputElement)
				.value,
		).toBe("Lena");
		const job = dialog.getByRole("combobox", { name: en.sheetJob });
		expect(
			within(job)
				.getAllByRole("option")
				.map((o) => o.textContent),
		).not.toContain("Product Manager");
		fireEvent.change(job, { target: { value: "architect" } });
		fireEvent.click(screen.getByRole("button", { name: en.sheetRemove }));
		expect(screen.queryByRole("dialog")).toBeNull();
		expect(rows()).toEqual(["Mira", "Ada", "Theo", "Kai"]);

		// Up to seven, then nothing more to add.
		for (let added = 0; added < 3; added++) {
			fireEvent.click(screen.getByRole("button", { name: en.teamAdd }));
			fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		}
		expect(rows()).toHaveLength(7);
		expect(new Set(rows()).size).toBe(7);
		expect(screen.queryByRole("button", { name: en.teamAdd })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		expect((await validatedTeam(s)).agents).toHaveLength(7);
		await s.reply(await asked(s, "team.validate"), {
			errors: [{ path: "/agents", message: "no", code: "too_many" }],
			effects: [],
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseTooMany,
		);
		expect(screen.queryByRole("heading", { name: en.mayTitle })).toBeNull();
	});

	it("carries_a_saved_teams_answers_to_spending_and_into_the_team_it_starts", async () => {
		const { container, s } = await picker([CODE], {
			...LISTED,
			templates: [THREE],
		});
		fireEvent.click(await screen.findByRole("radio", { name: /^Three of us/ }));
		expect(rows()).toEqual(["Mira", "Ada", "Theo"]);
		await expectNoAxeViolations(container);
		fireEvent.click(screen.getByRole("button", { name: en.teamContinue }));
		await s.reply(await asked(s, "team.validate"), noErrors);
		// What they may do is not asked again.
		await screen.findByRole("heading", { name: en.spendTitle });
		expect(screen.queryByText(en.wizardMay)).toBeNull();
		expect(
			(screen.getByLabelText(en.spendAmount) as HTMLInputElement).value,
		).toBe("20");
		fireEvent.click(screen.getByRole("button", { name: en.continue }));
		await screen.findByRole("heading", { name: en.finishTitle });
		await answerQuery(s, "team.get", { team: proposed().team, online: true });
		const carried = await screen.findByRole("region", {
			name: t("finishCarried", { name: "Three of us" }),
		});
		expect(carried.textContent).toContain(
			`${en.finishYes} ${en.finishCommandsYes}`,
		);
		expect(carried.textContent).toContain(`${en.finishNo} ${en.finishPushNo}`);
		// The spending step's link goes back to spending, never to the permissions.
		fireEvent.click(
			within(screen.getByRole("navigation", { name: en.setupSteps })).getByRole(
				"button",
				{ name: en.wizardSpending },
			),
		);
		await screen.findByRole("heading", { name: en.spendTitle });
		expect(screen.queryByRole("heading", { name: en.mayTitle })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: en.continue }));
		await screen.findByRole("heading", { name: en.finishTitle });

		fireEvent.click(screen.getByRole("button", { name: en.startTeam }));
		const start = (await sent(s, "team.start")).params as {
			team: {
				agents: Record<string, unknown>[];
				policy: Record<string, unknown>;
				budgets: unknown;
			};
		};
		expect(start.team.agents.map((a) => [a.id, a.persona, a.status])).toEqual([
			["mira", "Mira.", "active"],
			["ada", "Ada.", "active"],
			["theo", "Theo persona", "active"],
		]);
		expect(start.team.agents[0]?.model).toEqual({
			id: "claude-sonnet-5",
			effort: "low",
		});
		expect(start.team.policy.permissions).toEqual({
			run_commands: true,
			push: false,
		});
		expect(start.team.policy.integration).toBe("pull_request");
		expect(start.team.budgets).toEqual({ daily_usd: 20 });
		expect("solo" in start.team.policy).toBe(false);
	});

	it("builds_your_own_from_a_product_manager_and_a_developer_once_everyone_is_named", async () => {
		const { container, s } = await picker([CODE]);
		fireEvent.click(
			await screen.findByRole("radio", {
				name: new RegExp(`^${en.pickScratch}`),
			}),
		);
		expect(screen.getAllByText(en.whoNeedsName)).toHaveLength(2);
		const onward = screen.getByRole("button", {
			name: en.teamContinue,
		}) as HTMLButtonElement;
		expect(onward.disabled).toBe(true);
		expect(screen.getByText(en.teamNameAll)).toBeTruthy();
		await expectNoAxeViolations(container);

		const name = (who: string, value: string) => {
			fireEvent.click(
				screen
					.getAllByRole("button", { name: /^Change / })
					.find((b) => b.textContent?.includes(who)) as HTMLElement,
			);
			fireEvent.change(sheet().getByRole("textbox", { name: en.sheetName }), {
				target: { value },
			});
			fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		};
		name("Product Manager", "Mira");
		// The Product Manager every team needs cannot be taken off; the Developer can.
		fireEvent.click(screen.getByRole("button", { name: "Change Mira" }));
		expect(sheet().queryByRole("switch", { name: en.sheetOnTeam })).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: en.sheetDone }));
		expect(onward.disabled).toBe(true);
		name("Developer", "Theo");
		expect(onward.disabled).toBe(false);
		fireEvent.click(onward);
		expect((await validatedTeam(s)).agents.map((a) => a.role)).toEqual([
			"product_manager",
			"software_developer",
		]);
	});

	it("finish_says_what_happens_to_finished_work", async () => {
		const asFinish = async (online: boolean) => {
			const { socket } = await renderApp("/setup/finish");
			const s = socket as FakeSocket;
			await answerQuery(s, "team.propose", proposed());
			await answerStatus(s, false, 1, { setup_pending: true });
			await answerQuery(s, "team.get", { team: proposed().team, online });
			await screen.findByRole("heading", { name: en.finishTitle });
		};
		await asFinish(true);
		expect(await screen.findByText(en.finishLead)).toBeTruthy();
		expect(screen.queryByText(en.finishLeadLocal)).toBeNull();
		cleanup();
		await asFinish(false);
		expect(await screen.findByText(en.finishLeadLocal)).toBeTruthy();
		expect(screen.queryByText(en.finishLead)).toBeNull();
		cleanup();
	});
});
