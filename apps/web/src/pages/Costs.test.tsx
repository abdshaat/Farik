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
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	status: "active",
});
const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		agent("theo", "Theo", "software_developer", "developer"),
	],
	budgets: { max_task_usd: 5 },
	policy: { integration: "auto_merge" },
	rules: {},
};
const SUMMARY = {
	today_usd: 6.12,
	daily_limit_usd: null,
	sprint: { sprint_id: "S2", spent_usd: 11.84, budget_usd: 20 },
	agents: [
		{ agent_id: "mira", today_usd: 1.48, sprint_usd: 2.35 },
		{ agent_id: "theo", today_usd: 2.94, sprint_usd: 6.1 },
	],
};
const ACTIVITY = {
	activity: [
		{
			agent_id: "theo",
			state: "working",
			line: "Building FRK-7",
			task_id: "FRK-7",
		},
		{ agent_id: "mira", state: "idle", line: "Nothing to do yet" },
	],
};
const EMPTY = {
	accepted_tasks: 0,
	first_pass_acceptance_rate: null,
	interventions_per_accepted_task: null,
	cost_per_accepted_task: null,
	mechanically_verified_criteria_share: null,
	active_weeks: 0,
	messages: {
		reaction: 0,
		ambient: 0,
		reply: 0,
		ceremony: 0,
		system: 0,
		human: 0,
	},
};
const METRICS = {
	accepted_tasks: 9,
	first_pass_acceptance_rate: 6 / 9,
	interventions_per_accepted_task: 1.3,
	cost_per_accepted_task: {
		total_usd: 3.07,
		by_purpose: [
			{ words: "Planning", usd: 0.62 },
			{ words: "Building", usd: 1.88 },
			{ words: "Checking", usd: 0.41 },
			{ words: "Meetings and talk", usd: 0.16 },
		],
	},
	mechanically_verified_criteria_share: 0.71,
	active_weeks: 2,
	messages: {
		reaction: 4,
		ambient: 1,
		reply: 3,
		ceremony: 7,
		system: 2,
		human: 5,
	},
};

/** The Costs page, with each query answered. */
async function costs(summary: unknown = SUMMARY, metrics: unknown = EMPTY) {
	const { container, socket } = await renderApp("/costs");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "costs.summary", summary);
	await answerQuery(s, "team.activity", ACTIVITY);
	await answerQuery(s, "metrics", metrics);
	await screen.findByRole("heading", { level: 1, name: en.costs });
	return { container, s };
}

const metricsAsked = (s: FakeSocket) =>
	s.calls("query").filter((f) => f.params.name === "metrics");

describe("costs page", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_the_costs", async () => {
		const first = await costs();
		expect(
			screen.getByText(
				"Today the team has spent $6.12, with no daily limit set.",
			),
		).toBeTruthy();
		expect(
			screen.getByText("Sprint 2 has spent $11.84 of its $20.00 limit."),
		).toBeTruthy();

		// One row per agent: today, this sprint, and what it is doing right now.
		const table = screen.getByRole("table", { name: en.costsByAgent });
		const row = (name: string) =>
			within(table).getByText(name).closest("tr") as HTMLElement;
		expect(
			within(row("Theo"))
				.getAllByRole("cell")
				.map((c) => c.textContent),
		).toEqual(["$2.94", "$6.10", "Building FRK-7"]);
		expect(
			within(row("Mira"))
				.getAllByRole("cell")
				.map((c) => c.textContent),
		).toEqual(["$1.48", "$2.35", "Nothing to do yet"]);

		// With no work accepted, every rate says so.
		const how = screen.getByRole("region", { name: en.costsHowWell });
		expect(within(how).getAllByText(en.metricNotYet)).toHaveLength(4);
		expect(within(how).getByText("Active weeks: 0")).toBeTruthy();
		await expectNoAxeViolations(first.container);
		cleanup();

		// With a limit, the line names it; with numbers, the rates say them.
		await costs({ ...SUMMARY, daily_limit_usd: 10, sprint: null }, METRICS);
		expect(
			screen.getByText("Today the team has spent $6.12, of $10.00 a day."),
		).toBeTruthy();
		expect(screen.queryByText(/^Sprint 2 has spent/)).toBeNull();
		const rates = screen.getByRole("region", { name: en.costsHowWell });
		expect(within(rates).queryByText(en.metricNotYet)).toBeNull();
		expect(within(rates).getByText("6 of 9")).toBeTruthy();
		expect(within(rates).getByText("1.3")).toBeTruthy();
		expect(within(rates).getByText("$3.07")).toBeTruthy();
		expect(
			within(rates).getByText(
				"Planning $0.62, Building $1.88, Checking $0.41, Meetings and talk $0.16.",
			),
		).toBeTruthy();
		expect(within(rates).getByText("71%")).toBeTruthy();
		expect(within(rates).getByText("Active weeks: 2")).toBeTruthy();
		expect(
			within(rates).getByText(
				"Messages: 4 reactions, 3 replies, 7 from meetings, 5 from you",
			),
		).toBeTruthy();
	});

	it("sets_a_daily_limit", async () => {
		const { container, s } = await costs();
		fireEvent.click(screen.getByRole("button", { name: en.costsSetLimit }));
		const dialog = screen.getByRole("dialog", { name: en.costsSetLimit });
		await answerQuery(s, "settings.defaults", {
			budgets: { daily_usd: 15 },
			policy: { integration: "auto_merge" },
			rules: {},
		});
		// The setup's own control: no limit, or a daily one.
		expect(
			(
				within(dialog).getByRole("radio", {
					name: new RegExp(`^${en.spendNone}`),
				}) as HTMLInputElement
			).checked,
		).toBe(true);
		// "Put back the default" puts back the defaults' limit.
		const putBack = within(dialog).getByRole("button", {
			name: en.putBack,
		}) as HTMLButtonElement;
		await waitFor(() => expect(putBack.disabled).toBe(false));
		fireEvent.click(putBack);
		expect(
			(within(dialog).getByLabelText(en.spendAmount) as HTMLInputElement).value,
		).toBe("15");
		fireEvent.change(within(dialog).getByLabelText(en.spendAmount), {
			target: { value: "7.50" },
		});
		await expectNoAxeViolations(container);
		fireEvent.click(within(dialog).getByRole("button", { name: en.limitSave }));
		const frame = await waitFor(() => {
			const f = s.calls("team.save").at(-1);
			if (!f) throw new Error("no team.save was sent");
			return f;
		});
		const team = (frame.params as { team: typeof TEAM }).team;
		expect(team.budgets).toEqual({ max_task_usd: 5, daily_usd: 7.5 });
		expect(team.agents).toHaveLength(2);
		act(() => s.reply(frame, {}));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("shows_one_sprints_metrics", async () => {
		const { s } = await costs();
		expect(metricsAsked(s).at(-1)?.params.params).toEqual({});
		const toggle = screen.getByRole("button", {
			name: "Show sprint 2 only",
		});
		expect(toggle.getAttribute("aria-pressed")).toBe("false");
		expect(screen.getByText(en.costsWholeProject)).toBeTruthy();
		fireEvent.click(toggle);
		await waitFor(() =>
			expect(metricsAsked(s).at(-1)?.params.params).toEqual({
				sprint_id: "S2",
			}),
		);
		expect(toggle.getAttribute("aria-pressed")).toBe("true");
		expect(screen.getByText("In sprint 2 only.")).toBeTruthy();
		fireEvent.click(toggle);
		await waitFor(() =>
			expect(metricsAsked(s).at(-1)?.params.params).toEqual({}),
		);
	});
});
