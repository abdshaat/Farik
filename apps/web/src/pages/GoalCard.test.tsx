import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ConnectionProvider } from "../app/connection.tsx";
import {
	type FakeSocket,
	fakeFetch,
	socketsMade,
} from "../test/fake-socket.ts";
import { sentCommand } from "../test/gate.ts";
import { bodyOf, refusedBy } from "../test/schema.ts";
import {
	type Goal,
	GoalCard,
	type GoalState,
	guessTarget,
} from "./GoalCard.tsx";
import type { Team } from "./setup/TeamSetup.tsx";

const agent = (id: string, displayName: string, role: string, more = {}) => ({
	id,
	displayName,
	role,
	status: "active",
	...more,
});
const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager"),
		agent("finn", "Finn", "finance_specialist", {
			mcpServers: [{ name: "stripe", source: "custom" }],
		}),
		agent("theo", "Theo", "software_developer", {
			mcpServers: [{ name: "playwright", source: "builtin" }],
		}),
	],
	budgets: { dailyUsd: 10 },
	policy: { integration: "auto_merge" },
	rules: {},
} as unknown as Team;

const hoursAgo = (hours: number) =>
	new Date(Date.now() - hours * 3_600_000).toISOString();

const A_GOAL: Goal = {
	goal: "goal-1",
	words: "Reach 10 paying customers",
	target: 10,
	unit: "paying customers",
	server: "stripe",
	reader: "finn",
	planner: "mira",
	status: "active",
	stall: null,
	readings: [
		{
			value: 4,
			source: "connector",
			server: "stripe",
			agent: "finn",
			at: hoursAgo(3),
		},
	],
	open: 5,
	done: 3,
	waitingOnOwner: 0,
	planning: false,
};

/** The card, drawn with a connection, and the socket its commands go out on. */
async function drawn(state: GoalState) {
	vi.stubGlobal("fetch", vi.fn(fakeFetch({ "GET /session": 204 })));
	const { factory, sockets } = socketsMade();
	const again = vi.fn();
	const { container } = render(
		<ConnectionProvider socketFactory={factory}>
			<MemoryRouter initialEntries={["/"]}>
				<GoalCard state={state} team={TEAM} again={again} />
			</MemoryRouter>
		</ConnectionProvider>,
	);
	const s: FakeSocket = await waitFor(() => {
		const one = sockets[0];
		if (!one) throw new Error("no socket was opened");
		return one;
	});
	act(() => s.emit("open", {}));
	return { container, s, again };
}

const NONE: GoalState = { goal: null, lastEnded: null };

describe("the goal card", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
	});

	it("sets_a_goal_with_the_number_guessed_from_its_words", async () => {
		expect(
			guessTarget("Create a mobile application and reach 10k downloads"),
		).toEqual({ target: 10000, unit: "downloads" });
		expect(guessTarget("Reach $5,000 MRR by March")).toEqual({
			target: 5000,
			unit: "MRR",
		});
		expect(guessTarget("Make the app")).toBeNull();

		const { container, s, again } = await drawn(NONE);
		expect(await screen.findByText("Give your team a goal")).toBeTruthy();
		await expectNoAxeViolations(container);
		fireEvent.click(screen.getByRole("button", { name: "Set a goal" }));
		const dialog = await screen.findByRole("dialog");
		const inside = within(dialog);
		fireEvent.change(
			inside.getByRole("textbox", { name: "What should your team achieve?" }),
			{ target: { value: "Reach 10 paying customers" } },
		);
		expect(
			(
				inside.getByRole("textbox", {
					name: "A number to reach",
				}) as HTMLInputElement
			).value,
		).toBe("10");
		expect(
			(
				inside.getByRole("textbox", {
					name: "What it counts",
				}) as HTMLInputElement
			).value,
		).toBe("paying customers");
		// Counting by a built-in connector makes no sense; Finn's Stripe does.
		expect(inside.queryByRole("radio", { name: /playwright/i })).toBeNull();
		fireEvent.click(
			inside.getByRole("radio", { name: /From Stripe \(Finn reads it\)/ }),
		);
		fireEvent.click(inside.getByRole("button", { name: "Set goal" }));

		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "goal_set",
				body: {
					words: "Reach 10 paying customers",
					target: 10,
					unit: "paying customers",
					server: "stripe",
				},
			},
		});
		expect(refusedBy("goalSetBody", bodyOf(sent))).toEqual([]);
		await s.reply(sent, {
			error: {
				kind: "refused",
				detail:
					"goal_needs_daily_budget: set a daily budget first: a goal keeps the team working every day",
			},
		});
		const link = await screen.findByRole("link", {
			name: "Set a daily budget first: a goal keeps your team working every day.",
		});
		expect(link.getAttribute("href")).toBe("/settings/spending");
		expect(again).not.toHaveBeenCalled();

		fireEvent.click(inside.getByRole("button", { name: "Set goal" }));
		const second = await sentCommand(s, 2);
		await s.reply(second, { said: "Set the goal goal-1.", events: [9] });
		await waitFor(() => expect(again).toHaveBeenCalledTimes(1));
		expect(screen.queryByRole("dialog")).toBeNull();
	});

	it("shows_progress_and_asks_for_the_owners_word", async () => {
		const { s } = await drawn({ goal: A_GOAL, lastEnded: null });
		expect(screen.getByText("Reach 10 paying customers")).toBeTruthy();
		expect(screen.getByText("4 of 10 paying customers")).toBeTruthy();
		expect(
			screen.getByText("From Stripe, read by Finn 3 hours ago"),
		).toBeTruthy();
		expect(screen.getByText("5 tasks under way, 3 done")).toBeTruthy();
		cleanup();

		// The count has reached the target: the owner is asked, and Mark reached asks once more.
		const met: Goal = {
			...A_GOAL,
			status: "met",
			readings: [
				{
					value: 10,
					source: "connector",
					server: "stripe",
					agent: "finn",
					at: hoursAgo(1),
				},
			],
		};
		const reached = await drawn({ goal: met, lastEnded: null });
		expect(
			screen.getByText(
				"Your team counts 10 of 10 paying customers. Is the goal reached?",
			),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Mark reached" }));
		const dialog = await screen.findByRole("dialog");
		expect(
			within(dialog).getByText("Mark ‘Reach 10 paying customers’ reached?"),
		).toBeTruthy();
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Yes, mark reached" }),
		);
		const ended = await sentCommand(reached.s);
		expect(ended.params).toEqual({
			command: {
				command: "goal_end",
				body: { goal: "goal-1", outcome: "reached" },
			},
		});
		expect(refusedBy("goalEndBody", bodyOf(ended))).toEqual([]);
		cleanup();

		// A stalled goal says why, and Keep going is the owner's word.
		const stalled: Goal = {
			...A_GOAL,
			status: "stalled",
			stall: "no_progress",
		};
		const kept = await drawn({ goal: stalled, lastEnded: null });
		expect(
			screen.getByText("Five rounds of work and the count hasn't moved."),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Keep going" }));
		const resumed = await sentCommand(kept.s);
		expect(resumed.params).toEqual({
			command: { command: "goal_resume", body: { goal: "goal-1" } },
		});
		expect(refusedBy("goalIdBody", bodyOf(resumed))).toEqual([]);
		expect(s.calls("command")).toHaveLength(0);
		cleanup();

		// With no goal, the card is quiet, with the last one ended beneath it.
		await drawn({
			goal: null,
			lastEnded: {
				goal: "goal-1",
				words: "Reach 10 paying customers",
				outcome: "reached",
				at: "2026-10-19T09:00:00Z",
			},
		});
		expect(screen.getByRole("button", { name: "Set a goal" })).toBeTruthy();
		expect(
			screen.getByText(/Reached on .*: Reach 10 paying customers/),
		).toBeTruthy();
	});
});
