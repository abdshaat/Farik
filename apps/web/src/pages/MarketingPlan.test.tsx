import { expectNoAxeViolations } from "@farik/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { GOAL, PLAN, TEAM, TEXT } from "../test/marketing.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

/** The page of plan MP-3, with the team and `plan` answered. */
async function opened(plan: object = PLAN) {
	const { container, socket } = await renderApp("/marketing/plans/MP-3");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "marketing_plan.get", plan);
	return { container, s };
}

/** The command the page sent, once it has sent `count`. */
const sent = (s: FakeSocket, count = 1) =>
	waitFor(() => {
		const c = s.calls("command")[count - 1];
		if (!c) throw new Error(`fewer than ${count} commands were sent`);
		return c;
	});

/** A row's cells, each as its text, joined by a bar. */
const cells = (row: HTMLElement) =>
	within(row)
		.getAllByRole("cell")
		.map((c) => c.textContent)
		.join("|");

const APPROVED = {
	...PLAN,
	state: "active",
	decided: {
		decision: "approved",
		note: "Looks good.",
		at: "2026-10-06T19:05:00Z",
	},
};

describe("marketing plan page", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("the_page_shows_the_budget_calendar_and_text_as_text", async () => {
		const { container, s } = await opened();
		expect(
			await screen.findByRole("heading", {
				level: 1,
				name: "Autumn at Corner Bakery",
			}),
		).toBeTruthy();
		expect(
			s.calls("query").find((q) => q.params.name === "marketing_plan.get")
				?.params.params,
		).toEqual({ plan: "MP-3" });
		expect(screen.getByText("Waiting on you")).toBeTruthy();
		expect(
			screen.getByText(
				"Marketing plan MP-3. Nothing in it is posted or spent until you approve it.",
			),
		).toBeTruthy();
		expect(
			screen.getByRole("link", { name: en.backToToday }).getAttribute("href"),
		).toBe("/");

		// The letter, signed by the agent that wrote it.
		const letter = screen.getByRole("region", {
			name: "Kai, your Marketing Specialist, wrote this for you",
		});
		expect(within(letter).getByText(PLAN.summary)).toBeTruthy();

		// What approving allows, in this plan's own numbers.
		const allows = screen.getByRole("region", {
			name: "What approving lets Kai do",
		});
		expect(
			within(allows).getByText(
				"Post the 6 posts below, each on its day, without asking you each time.",
			),
		).toBeTruthy();
		expect(
			within(allows).getByText(/^Spend up to \$450\.00 on the 2/),
		).toBeTruthy();

		// The budget, by channel and campaign, in dollars with its total.
		const budget = screen.getByRole("table", {
			name: "Budget by channel and campaign",
		});
		const rows = within(budget).getAllByRole("row");
		expect(
			within(rows[0] as HTMLElement)
				.getAllByRole("columnheader")
				.map((h) => h.textContent),
		).toEqual(["Channel and campaign", "Dates", "Budget"]);
		expect(cells(rows[1] as HTMLElement)).toBe(
			"Google Ads2 campaigns|12 Oct to 22 Nov|$450.00",
		);
		expect(cells(rows[2] as HTMLElement)).toBe(
			`Pie pre-orders${GOAL}|26 Oct to 22 Nov|$300.00`,
		);
		expect(cells(rows[3] as HTMLElement)).toBe(
			"Bakery near meNew customers searching for a bakery within 2 miles|12 Oct to 22 Nov|$150.00",
		);
		expect(cells(rows[4] as HTMLElement)).toBe(
			"Instagram4 posts|12 Oct to 26 Oct|No cost",
		);
		expect(cells(rows[5] as HTMLElement)).toBe(
			"X2 posts|14 Oct to 25 Oct|No cost",
		);
		expect(rows[6]?.textContent).toBe("Total$450.00 USD");
		expect(
			screen.getByText(
				"In US dollars, the currency of your Google Ads account.",
			),
		).toBeTruthy();

		// The posts, week by week, each with its day, its network and its topic.
		expect(
			screen.getByRole("heading", { name: "Posts, week by week" }),
		).toBeTruthy();
		const week = (name: string) =>
			within(screen.getByRole("list", { name })).getAllByRole("listitem");
		expect(
			week("Week 1: 12 to 18 October").map((li) => li.textContent),
		).toEqual([
			"Mon 12 OctInstagramOur autumn menu",
			"Wed 14 OctXPumpkin loaf is back",
			"Sat 17 OctInstagramShaping the sourdough",
		]);
		expect(week("Week 2: 19 to 25 October")).toHaveLength(2);
		expect(
			week("Week 3: 26 October to 1 November").map((li) => li.textContent),
		).toEqual(["Mon 26 OctInstagramPie pre-orders open"]);

		// How Kai will know it worked.
		const measures = screen.getByRole("list", {
			name: "How Kai will know it worked",
		});
		expect(
			within(measures)
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual(PLAN.measures);

		// About the plan: where it came from and what it covers.
		const about = screen.getByRole("region", { name: "About this plan" });
		for (const text of [
			"MP-3",
			"Monday 12 October to Sunday 22 November 2026: 6 weeks",
			"$450.00 USD",
			"2 campaigns, Google Ads account 482-193-7720",
			"6: 4 on Instagram, 2 on X",
			"Tuesday 6 October at 08:40",
		])
			expect(within(about).getByText(text)).toBeTruthy();
		expect(
			within(about).getByRole("link", { name: "FRK-31" }).getAttribute("href"),
		).toBe("/tasks/FRK-31");

		// Everything the agent wrote is text: its markup and its markdown are shown as typed.
		const whole = screen.getByRole("region", {
			name: "Kai’s whole plan, as written",
		});
		expect(whole.querySelector("pre")?.textContent).toBe(TEXT);
		// Not one element came out of it: the markup, and the markdown's bold and heading.
		expect(whole.querySelector("pre")?.children).toHaveLength(0);
		expect(container.querySelector("img, script, i, b")).toBeNull();
		expect(container.querySelectorAll("h1")).toHaveLength(1);
		expect(screen.queryByRole("heading", { name: "What I found" })).toBeNull();
		expect(within(budget).getByText(GOAL)).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("approve_sends_the_decision", async () => {
		const { container, s } = await opened();
		expect(
			screen.getByText(
				"Approving lets Kai post these 6 posts and spend up to $450.00 on these ads, without asking you each time.",
			),
		).toBeTruthy();
		fireEvent.click(
			await screen.findByRole("button", { name: "Approve the plan" }),
		);
		const approve = await sent(s);
		expect(approve.params).toEqual({
			command: {
				command: "marketing_plan_decide",
				body: { plan: "MP-3", decision: "approve" },
			},
		});
		// A refusal is said in words, whatever the daemon's text is.
		await s.reply(approve, {
			error: {
				kind: "refused",
				detail: "marketing_plan_decided: MP-3 was decided already",
			},
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseMarketingPlanDecided,
		);
		await expectNoAxeViolations(container);
	});

	it("send_back_needs_a_reason", async () => {
		const { container, s } = await opened();
		fireEvent.click(await screen.findByRole("button", { name: "Send back" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Send the plan back to Kai",
		});
		expect(
			within(dialog).getByText(
				"Kai reads your words in its next session and writes a new version for you to approve. Nothing is posted or spent meanwhile.",
			),
		).toBeTruthy();
		const send = within(dialog).getByRole("button", { name: "Send it back" });
		expect((send as HTMLButtonElement).disabled).toBe(true);
		// Blank words are no reason.
		const why = within(dialog).getByLabelText(/What should change\?/);
		fireEvent.change(why, { target: { value: "   " } });
		expect((send as HTMLButtonElement).disabled).toBe(true);
		expect(within(dialog).getByText("0 of 600")).toBeTruthy();

		// More than the 600 characters the daemon takes cannot be sent.
		fireEvent.change(why, { target: { value: "x".repeat(601) } });
		expect((send as HTMLButtonElement).disabled).toBe(true);
		expect(within(dialog).getByText("601 of 600")).toBeTruthy();
		expect(s.calls("command")).toHaveLength(0);

		const reason = "Halve the Google Ads budget for October: we close.";
		fireEvent.change(why, { target: { value: ` ${reason} ` } });
		expect((send as HTMLButtonElement).disabled).toBe(false);
		await expectNoAxeViolations(container);
		fireEvent.click(send);
		const back = await sent(s);
		expect(back.params).toEqual({
			command: {
				command: "marketing_plan_decide",
				body: { plan: "MP-3", decision: "return", note: reason },
			},
		});
		await s.reply(back, { said: "sent back", events: [9] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("end_asks_first_then_sends", async () => {
		// Day 23 of 42: the 3rd of November.
		vi.useFakeTimers({ toFake: ["Date"] });
		vi.setSystemTime(new Date("2026-11-03T09:00:00Z"));
		const { container, s } = await opened(APPROVED);
		expect(
			await screen.findByText(
				"Marketing plan MP-3. Day 23 of 42: it ends on Sunday 22 November.",
			),
		).toBeTruthy();
		expect(screen.getByText("Running")).toBeTruthy();
		expect(
			screen.getByText("You approved this plan on Tuesday 6 October at 19:05"),
		).toBeTruthy();
		expect(screen.getByText("“Looks good.”")).toBeTruthy();
		// A decided plan has no decision to make.
		for (const name of ["Approve the plan", "Send back"])
			expect(screen.queryByRole("button", { name })).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: "End the plan" }));
		const dialog = await screen.findByRole("dialog", {
			name: "End this plan now?",
		});
		expect(
			within(dialog).getByText(
				"Autumn at Corner Bakery, MP-3, on day 23 of 42.",
			),
		).toBeTruthy();
		// Asking first: nothing is sent until it is confirmed.
		expect(s.calls("command")).toHaveLength(0);
		fireEvent.change(within(dialog).getByLabelText(/A note for Kai/), {
			target: { value: "We close early for the refit." },
		});
		await expectNoAxeViolations(container);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "End the plan" }),
		);
		const end = await sent(s);
		expect(end.params).toEqual({
			command: {
				command: "marketing_plan_end",
				body: { plan: "MP-3", note: "We close early for the refit." },
			},
		});
		await s.reply(end, { said: "ended", events: [10] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("ends_a_plan_without_a_note_and_keeps_it_when_not_confirmed", async () => {
		vi.useFakeTimers({ toFake: ["Date"] });
		vi.setSystemTime(new Date("2026-10-08T09:00:00Z"));
		const { s } = await opened({ ...APPROVED, state: "approved" });
		// Before its first day the plan says so, and ending it is already offered.
		expect(
			await screen.findByText("Approved: starts Monday 12 October"),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "End the plan" }));
		const dialog = await screen.findByRole("dialog", {
			name: "End this plan now?",
		});
		expect(
			within(dialog).getByText(
				"Autumn at Corner Bakery, MP-3, which has not started.",
			),
		).toBeTruthy();
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Keep the plan" }),
		);
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);

		fireEvent.click(screen.getByRole("button", { name: "End the plan" }));
		fireEvent.click(
			within(await screen.findByRole("dialog")).getByRole("button", {
				name: "End the plan",
			}),
		);
		expect((await sent(s)).params).toEqual({
			command: { command: "marketing_plan_end", body: { plan: "MP-3" } },
		});
	});

	it("says_what_became_of_a_plan_the_owner_decided_or_that_ended", async () => {
		const returned = {
			...PLAN,
			state: "returned",
			decided: {
				decision: "returned",
				reason: "Halve the budget <b>now</b>.",
				at: "2026-10-06T10:12:00Z",
			},
		};
		const first = await opened(returned);
		expect((await screen.findAllByText("Sent back")).length).toBeGreaterThan(0);
		expect(
			screen.getByText("You sent this plan back on Tuesday 6 October at 10:12"),
		).toBeTruthy();
		// The owner's own words, as typed.
		expect(screen.getByText("“Halve the budget <b>now</b>.”")).toBeTruthy();
		expect(
			screen.getByText(
				"Kai is writing a new version. It will wait for you on Today when it is ready.",
			),
		).toBeTruthy();
		for (const name of ["Approve the plan", "Send back", "End the plan"])
			expect(screen.queryByRole("button", { name })).toBeNull();
		await expectNoAxeViolations(first.container);
		cleanup();

		const ended = (why: string, at: string, more: object = {}) => ({
			...APPROVED,
			state: "ended",
			ended: { why, at, ...more },
		});
		const cases: [string, string, string, object][] = [
			[
				"expired",
				"2026-11-22T00:00:01Z",
				"This plan ended on Sunday 22 November, its last day",
				{},
			],
			[
				"by_owner",
				"2026-11-03T09:41:00Z",
				"You ended this plan on Tuesday 3 November at 09:41",
				{},
			],
			[
				"replaced",
				"2026-11-16T00:00:01Z",
				"A newer plan took over from this one on Monday 16 November",
				{},
			],
		];
		for (const [why, at, line, more] of cases) {
			await opened(ended(why, at, more));
			expect(await screen.findByText(line)).toBeTruthy();
			expect(screen.getAllByText("Ended").length).toBeGreaterThan(0);
			expect(
				screen.getByText(
					"You approved it on Tuesday 6 October. Kai cannot post or advertise for it any more.",
				),
			).toBeTruthy();
			expect(screen.queryByRole("button", { name: "End the plan" })).toBeNull();
			cleanup();
		}

		// An owner's end keeps their words, shown as typed.
		const owners = await opened(
			ended("by_owner", "2026-11-03T09:41:00Z", {
				note: "We close early <b>for</b> the refit.",
			}),
		);
		expect(
			await screen.findByText(
				"You ended this plan on Tuesday 3 November at 09:41",
			),
		).toBeTruthy();
		expect(
			screen.getByText("“We close early <b>for</b> the refit.”"),
		).toBeTruthy();
		expect(owners.container.querySelector("b")).toBeNull();
		cleanup();

		// A replaced plan names the plan that took over, and links to it.
		await opened(
			ended("replaced", "2026-11-16T00:00:01Z", { replaced_by: "MP-5" }),
		);
		const took = await screen.findByRole("link", { name: "MP-5" });
		expect(took.getAttribute("href")).toBe("/marketing/plans/MP-5");
		expect(took.closest("p")?.textContent).toBe(
			"This plan ended on Monday 16 November, when MP-5 took over",
		);
		expect(screen.queryByText(/A newer plan took over/)).toBeNull();
	});

	it("says_when_there_is_no_such_plan", async () => {
		const { socket } = await renderApp("/marketing/plans/MP-9");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", { team: TEAM });
		const asked = s
			.calls("query")
			.find((q) => q.params.name === "marketing_plan.get");
		await s.fail(asked as never, -32002, "not_found: no plan MP-9");
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.pageNotFound,
		);
	});
});
