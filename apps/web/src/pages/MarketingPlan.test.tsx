import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { RUNNING_PLAN } from "../test/ads.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import {
	ADVERTISES,
	GOAL,
	NAME,
	PLAN,
	SUMMARY,
	TEAM,
	TEXT,
	TITLE,
	TOPIC,
} from "../test/marketing.ts";
import { at } from "../test/posts.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
import own from "./MarketingPlan.module.css";

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
				name: TITLE,
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
		expect(within(letter).getByText(SUMMARY)).toBeTruthy();
		// With the agent's own picture, as Today's row has it.
		expect(within(letter).getByRole("img", { name: "Kai" })).toBeTruthy();

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
			`${NAME}${GOAL}Advertises: ${ADVERTISES}Price: fixed at $300.00 USD|26 Oct to 22 Nov|$300.00`,
		);
		expect(cells(rows[3] as HTMLElement)).toBe(
			"Bakery near meNew customers searching for a bakery within 2 miles" +
				"Advertises: The bakery itself: bread and pastries fresh from 7 amPrice: fixed at $150.00 USD|12 Oct to 22 Nov|$150.00",
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
			`Mon 12 OctInstagram${TOPIC}`,
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

		// About the plan: where it came from and what it covers, in a side column on a wide
		// screen, and ahead of the summary on a phone, where the page is one column.
		const about = screen.getByRole("complementary", {
			name: "About this plan",
		});
		expect(
			about.compareDocumentPosition(letter) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
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
		// Every text the agent writes carries a `<b>`: the title, the summary, a campaign's name and
		// goal, a post's topic and a measure. Each was compared above as typed, so none is an element.
		expect(container.querySelector("script, i, b")).toBeNull();
		// The only picture is the agent's own: the `img` in its text is shown, not made.
		expect(
			[...container.querySelectorAll("img")].map((img) => img.alt),
		).toEqual(["Kai"]);
		expect(container.querySelectorAll("h1")).toHaveLength(1);
		expect(screen.queryByRole("heading", { name: "What I found" })).toBeNull();
		expect(within(budget).getByText(GOAL)).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("keeps_the_decision_bar_above_the_phones_tab_bar", () => {
		// jsdom has no layout, so the rule is read from the style sheets: the shell's narrow layout
		// says how tall its sticky tab bar is, and the page's own sticky bar rests above it.
		const css = (file: string) =>
			readFileSync(join(import.meta.dirname, file), "utf8").replace(
				/\s+/g,
				" ",
			);
		expect(css("MarketingPlan.module.css")).toMatch(
			/\.bar \{[^}]*position: sticky;[^}]*bottom: var\(--catervas-shell-bar-height, 0\);/,
		);
		const shell = css("../shell/Shell.module.css");
		expect(shell).toMatch(
			/\.narrow \{[^}]*--catervas-shell-bar-height: [^;]+;/,
		);
		expect(shell).toMatch(
			/\.bar \{[^}]*height: var\(--catervas-shell-bar-height\);/,
		);
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
			within(dialog).getByText(`${TITLE}, MP-3, on day 23 of 42.`),
		).toBeTruthy();
		// Asking first: nothing is sent until it is confirmed.
		expect(s.calls("command")).toHaveLength(0);
		const note = within(dialog).getByLabelText(/A note for Kai/);
		const end = within(dialog).getByRole("button", { name: "End the plan" });
		// A note is optional, but more than the 600 characters the daemon takes cannot be sent.
		expect((end as HTMLButtonElement).disabled).toBe(false);
		fireEvent.change(note, { target: { value: "x".repeat(601) } });
		expect(within(dialog).getByText("601 of 600")).toBeTruthy();
		expect((end as HTMLButtonElement).disabled).toBe(true);
		expect(s.calls("command")).toHaveLength(0);
		fireEvent.change(note, { target: { value: "x".repeat(600) } });
		expect(within(dialog).getByText("600 of 600")).toBeTruthy();
		expect((end as HTMLButtonElement).disabled).toBe(false);
		fireEvent.change(note, {
			target: { value: "We close early for the refit." },
		});
		expect((end as HTMLButtonElement).disabled).toBe(false);
		await expectNoAxeViolations(container);
		fireEvent.click(end);
		const ending = await sent(s);
		expect(ending.params).toEqual({
			command: {
				command: "marketing_plan_end",
				body: { plan: "MP-3", note: "We close early for the refit." },
			},
		});
		await s.reply(ending, { said: "ended", events: [10] });
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
			within(dialog).getByText(`${TITLE}, MP-3, which has not started.`),
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

/** A post written for one of MP-3's slots, as `marketing_plan.get` lists it. */
const written = (post: number, slot: string, more: object) => ({
	post,
	slot,
	text: `The post for ${slot}.`,
	at: at(12, 10),
	state: "sent",
	state_at: at(12, 8),
	...more,
});

describe("a plan's posts", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	/** The slot of the calendar whose topic says `topic`. */
	const slot = (topic: string) => {
		const item = screen
			.getAllByRole("listitem")
			.find((one) => one.textContent?.includes(topic));
		if (!item) throw new Error(`no slot says "${topic}"`);
		return item;
	};

	it("the_plan_page_shows_each_slot_s_post", async () => {
		// Sunday 25 October, 09:15: the plan's slots p1 to p5 are today or behind it.
		vi.useFakeTimers({ toFake: ["Date"] });
		vi.setSystemTime(new Date(2026, 9, 25, 9, 15));
		const { container } = await opened({
			...APPROVED,
			written_posts: [
				written(11, "p1", {
					text: "Our autumn menu, first try.",
					state: "failed",
					at: at(12, 10),
					state_at: at(12, 8, 5),
				}),
				written(12, "p1", {
					text: `Our autumn menu ${"<b>x</b>"}`,
					state: "sent",
					at: at(12, 10),
				}),
				written(13, "p2", {
					state: "stopped",
					stopped_by: "owner",
					state_at: at(13, 17),
				}),
				written(14, "p3", { state: "missed", missed_why: "paused" }),
				written(15, "p4", { state: "missed", missed_why: "not_running" }),
				written(16, "p5", {
					state: "scheduled",
					at: at(25, 18),
					text: "A rainy week ahead.",
				}),
				// Stopped, and written again, and that one failed: the slot has none.
				written(17, "p6", {
					state: "stopped",
					stopped_by: "owner",
					state_at: at(23, 17),
				}),
				written(18, "p6", { state: "failed", state_at: at(24, 7, 45) }),
			],
		});
		await screen.findByRole("heading", {
			level: 2,
			name: "Posts, week by week",
		});

		// Each slot says where its post stands, with the post's own words.
		const sent = slot(TOPIC);
		// The state is a column of its own, at the row's right end, with the day or time under it.
		const state = within(sent).getByText("Sent").parentElement as HTMLElement;
		expect(state.classList).toContain(own.slotState);
		expect(state.textContent).toBe("Sent at 10:00");
		expect(sent.lastElementChild).toBe(state);
		// The post's words are cut after one line, whole for a reader of the page.
		const words = sent.querySelector(`.${own.slotText}`);
		expect(words?.textContent).toBe("Our autumn menu <b>x</b>");
		expect(sent.textContent).toContain("Sent");
		expect(sent.textContent).toContain("at 10:00");
		expect(sent.textContent).toContain("Our autumn menu <b>x</b>");
		expect(sent.querySelector("b")).toBeNull();
		expect(sent.textContent).toContain(
			"An earlier post for this day failed at 08:05.",
		);
		expect(slot("Pumpkin loaf is back").textContent).toContain(
			"Stopped by you on Tue 13 Oct",
		);
		expect(slot("Shaping the sourdough").textContent).toContain(
			"Missed the team was paused",
		);
		expect(slot("Meet the bakers").textContent).toContain(
			"Missed Catervas could not hand it to Buffer before its time.",
		);
		expect(slot("A rainy week ahead").textContent).toContain(
			"Going out today at 18:00",
		);
		// A slot whose only post failed has none: the failure is told beside it.
		expect(slot("Pie pre-orders open").textContent).toContain("No post yet");
		expect(slot("Pie pre-orders open").textContent).toContain(
			"An earlier post for this day failed at 07:45.",
		);

		// The counts, a count of none left out, and what Sent means.
		expect(
			screen.getByText(
				"1 sent, 1 going out, 1 stopped by you, 2 missed, and 1 not written yet.",
			),
		).toBeTruthy();
		expect(
			screen.getByText(
				"Sent means Catervas handed the post to Buffer for its time; you stop a post on Today.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("says_how_a_plan_that_ended_stopped_its_posts", async () => {
		vi.useFakeTimers({ toFake: ["Date"] });
		vi.setSystemTime(new Date(2026, 10, 3, 9, 0));
		await opened({
			...APPROVED,
			state: "ended",
			ended: { why: "by_owner", at: "2026-11-03T08:41:00Z" },
			written_posts: [
				written(21, "p6", { state: "stopped", stopped_by: "plan_ended" }),
				written(22, "p5", { state: "sent" }),
			],
		});
		await screen.findByRole("heading", {
			level: 2,
			name: "Posts, week by week",
		});

		expect(slot("Pie pre-orders open").textContent).toContain(
			"Stopped when the plan ended",
		);
		expect(slot("A rainy week ahead").textContent).toContain("Sent");
		// A plan that ended has nothing going out, and only what it counted.
		expect(screen.getByText("1 sent and 4 not written yet.")).toBeTruthy();
		cleanup();

		// A running plan with nothing written yet says so in one clause.
		await opened({ ...APPROVED, written_posts: [] });
		expect(await screen.findByText("6 not written yet.")).toBeTruthy();
	});

	it("ending_the_plan_counts_its_posts", async () => {
		// Monday 26 October, 09:15.
		vi.useFakeTimers({ toFake: ["Date"] });
		vi.setSystemTime(new Date(2026, 9, 26, 9, 15));
		const posts = [
			written(31, "p1", { state: "scheduled", at: at(28, 10) }),
			written(32, "p2", { state: "scheduled", at: at(29, 10) }),
			written(33, "p3", { state: "scheduled", at: at(30, 10) }),
			written(34, "p4", { state: "sent", at: at(26, 10) }),
			// Out already: it is Buffer's to post, and no longer ours to stop.
			written(35, "p5", { state: "sent", at: at(26, 8) }),
		];
		const ask = async (written_posts: object[]) => {
			const { s } = await opened({ ...APPROVED, written_posts });
			fireEvent.click(
				await screen.findByRole("button", { name: "End the plan" }),
			);
			const dialog = await screen.findByRole("dialog", {
				name: "End this plan now?",
			});
			return { dialog, s };
		};

		// Three posts not yet sent will not go out; one is with Buffer and goes out today.
		const first = await ask(posts);
		expect(
			within(first.dialog).getByText(
				"Its 3 posts not yet sent will not go out.",
			),
		).toBeTruthy();
		expect(
			within(first.dialog).getByText(
				"1 post is already with Buffer and goes out today at 10:00. Stop it on Today if you do not want it.",
			),
		).toBeTruthy();
		cleanup();

		// One of each kind, and the other way round: several with Buffer, the first named.
		const second = await ask([
			posts[0] as object,
			written(35, "p4", { state: "sent", at: at(27, 8, 30) }),
			written(36, "p5", { state: "sent", at: at(28, 8, 30) }),
		]);
		expect(
			within(second.dialog).getByText(
				"Its 1 post not yet sent will not go out.",
			),
		).toBeTruthy();
		expect(
			within(second.dialog).getByText(
				"2 posts are already with Buffer, the first going out tomorrow at 08:30. Stop them on Today if you do not want them.",
			),
		).toBeTruthy();
		cleanup();

		// A line of none is left out.
		const third = await ask([
			written(37, "p1", { state: "sent", at: at(28, 8, 30) }),
		]);
		expect(within(third.dialog).queryByText(/not yet sent/)).toBeNull();
		expect(
			within(third.dialog).getByText(
				"1 post is already with Buffer and goes out Wednesday 28 October at 08:30. Stop it on Today if you do not want it.",
			),
		).toBeTruthy();
		cleanup();
		const none = await ask([]);
		expect(within(none.dialog).queryByText(/not yet sent/)).toBeNull();
		expect(within(none.dialog).queryByText(/with Buffer/)).toBeNull();
	});
});

describe("a plan's ads and their budget", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	/** The page of MP-3 on 12 November at 11:00 UTC, day 32 of 42, with `plan` answered. */
	async function onThe12th(plan: object) {
		vi.useFakeTimers({ toFake: ["Date"] });
		vi.setSystemTime(new Date("2026-11-12T11:00:00Z"));
		return opened(plan);
	}

	const budgetTable = () =>
		screen.getByRole("table", { name: "Budget by channel and campaign" });

	it("the_plan_shows_what_each_campaign_advertises_and_its_price", async () => {
		const daily = {
			...PLAN,
			campaigns: [
				PLAN.campaigns[0],
				{ ...PLAN.campaigns[1], price: "not_fixed" },
			],
		};
		const { container } = await opened(daily);
		await screen.findByRole("heading", { level: 1, name: TITLE });
		const rows = within(budgetTable()).getAllByRole("row");
		// What the agent says it advertises is shown as typed, and the price as the owner reads it.
		expect(cells(rows[2] as HTMLElement)).toContain(
			`Advertises: ${ADVERTISES}Price: fixed at $300.00 USD`,
		);
		expect(cells(rows[3] as HTMLElement)).toContain(
			"Price: up to $150.00 USD, may run over by about an hour’s spend",
		);
		expect(container.querySelector("script, i, b")).toBeNull();
		// Why a price is not fixed, once, under the table; no day was fixed yet.
		expect(
			screen.getByText(
				"A fixed price is a total that Google itself never charges past. Google keeps one only for a campaign of 3 to 90 days; any other has a daily budget, and since Google reports cost up to about an hour late, Catervas may pause it after about an hour’s more spend.",
			),
		).toBeTruthy();
		expect(screen.queryByText(/Prices as on the day you approved/)).toBeNull();
		// Approving says so too: in what it lets Kai do, and in the bar.
		const allows = screen.getByRole("region", {
			name: "What approving lets Kai do",
		});
		expect(allows.textContent).toContain(
			"Catervas pauses a campaign when it reaches its budget. Bakery near me is not at a fixed price, so it may run over by about an hour’s spend.",
		);
		expect(
			screen.getByText(/^Approving lets Kai post these 6 posts and spend up to/)
				.textContent,
		).toContain(" 1 campaign may run over by about an hour’s spend.");
		await expectNoAxeViolations(container);
	});

	it("a_plan_with_only_fixed_prices_says_nothing_of_running_over", async () => {
		await opened();
		await screen.findByRole("heading", { level: 1, name: TITLE });
		expect(screen.queryByText(/may run over/)).toBeNull();
		expect(screen.queryByText(/A fixed price is a total/)).toBeNull();
		expect(
			cells(within(budgetTable()).getAllByRole("row")[3] as HTMLElement),
		).toContain("Price: fixed at $150.00 USD");
	});

	it("a_plan_proposed_before_campaigns_said_what_they_advertise_says_not_stated", async () => {
		const older = {
			...PLAN,
			campaigns: PLAN.campaigns.map((one) => ({ ...one, advertises: "" })),
		};
		await opened(older);
		await screen.findByRole("heading", { level: 1, name: TITLE });
		expect(
			within(budgetTable()).getAllByText("Advertises:", { exact: false }),
		).toHaveLength(2);
		expect(
			cells(within(budgetTable()).getAllByRole("row")[2] as HTMLElement),
		).toContain("Advertises: Not stated");
	});

	it("an_approved_plan_says_its_prices_are_as_on_the_day_it_was_approved", async () => {
		await onThe12th({ ...RUNNING_PLAN, spend: undefined, paused: [] });
		await screen.findByRole("heading", { level: 1, name: TITLE });
		expect(
			screen.getByText(
				"In US dollars, the currency of your Google Ads account. Prices as on the day you approved the plan.",
			),
		).toBeTruthy();
	});

	it("the_plan_page_shows_spend_against_budget", async () => {
		const { container } = await onThe12th(RUNNING_PLAN);
		const spent = await screen.findByRole("region", { name: "Spent so far" });
		expect(spent.textContent).toContain("$318.65 of $450.00 USD");
		expect(
			within(spent).getByRole("img", {
				name: "71 per cent of the budget spent",
			}),
		).toBeTruthy();
		expect(
			within(spent).getByText(
				"Google Ads’ own figures, read today at 10:15. Catervas reads them every 15 minutes while it runs.",
			),
		).toBeTruthy();
		// A failed read is not shown while the last one worked.
		expect(within(spent).queryByRole("status")).toBeNull();

		// Each campaign against its budget, and where it stands: Catervas paused Bakery near me.
		const rows = within(budgetTable()).getAllByRole("row");
		expect(
			within(rows[0] as HTMLElement)
				.getAllByRole("columnheader")
				.map((h) => h.textContent),
		).toEqual(["Channel and campaign", "Dates", "Budget", "Spent so far"]);
		expect(cells(rows[1] as HTMLElement)).toBe(
			"Google Ads2 campaigns|12 Oct to 22 Nov|$450.00|$318.65",
		);
		expect(cells(rows[2] as HTMLElement)).toBe(
			`${NAME}${GOAL}Advertises: ${ADVERTISES}Price: fixed at $300.00 USD|26 Oct to 22 Nov|$300.00|$168.65`,
		);
		expect(cells(rows[3] as HTMLElement)).toBe(
			"Bakery near mePaused at its budgetNew customers searching for a bakery within 2 miles" +
				"Advertises: The bakery itself: bread and pastries fresh from 7 amPrice: fixed at $150.00 USD|12 Oct to 22 Nov|$150.00|$150.00",
		);
		expect(cells(rows[4] as HTMLElement)).toBe(
			"Instagram4 posts|12 Oct to 26 Oct|No cost|No cost",
		);
		expect(rows.at(-1)?.textContent).toBe("Total$450.00 USD$318.65");

		// Each pause Catervas made, with its time and why.
		const paused = screen.getByRole("region", { name: "Ads Catervas paused" });
		expect(
			within(paused)
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual([
			"Thu 12 Nov, 10:15Bakery near me: it reached its budget, $150.00 of $150.00 USD.",
		]);
		await expectNoAxeViolations(container);
	});

	it("the_plan_page_says_when_the_spend_cannot_be_read", async () => {
		await onThe12th({
			...RUNNING_PLAN,
			spend: {
				...RUNNING_PLAN.spend,
				failed:
					"Kai's sign-in to Google has ended; sign Kai in again on Kai's page",
				failed_at: "2026-11-12T10:45:00Z",
			},
		});
		const spent = await screen.findByRole("region", { name: "Spent so far" });
		// The last spend Catervas read stays, with when, and the failure is said beside it.
		expect(spent.textContent).toContain("$318.65 of $450.00 USD");
		expect(
			within(spent).getByText("Google Ads’ own figures, read today at 10:15."),
		).toBeTruthy();
		expect(within(spent).getByRole("status").textContent).toBe(
			"Catervas can’t read the spend now: Kai's sign-in to Google has ended; sign Kai in again on Kai's page. Last tried today at 10:45; Catervas tries again every 15 minutes. Until a read works, Catervas cannot pause the ads at their budget.",
		);
	});

	it("an_ended_plan_shows_the_last_spend_and_why_each_ad_is_paused", async () => {
		await onThe12th({
			...RUNNING_PLAN,
			state: "ended",
			ended: { why: "by_owner", at: "2026-11-12T10:40:00Z" },
			spend: {
				...RUNNING_PLAN.spend,
				read_at: "2026-11-12T10:30:00Z",
				failed: "Google is down",
				failed_at: "2026-11-12T10:32:00Z",
			},
			paused: [
				...RUNNING_PLAN.paused,
				{
					key: "pies",
					name: NAME,
					why: "plan_ended",
					at: "2026-11-12T10:41:00Z",
				},
			],
		});
		const spent = await screen.findByRole("region", { name: "Spent" });
		expect(
			within(spent).getByText(
				"Google Ads’ own figures, last read today at 10:30, before the plan ended. Google Ads itself has the final figures.",
			),
		).toBeTruthy();
		// A read that failed before the plan ended is not a warning any more: nothing is watched.
		expect(within(spent).queryByRole("status")).toBeNull();
		const rows = within(budgetTable()).getAllByRole("row");
		expect(
			within(rows[0] as HTMLElement)
				.getAllByRole("columnheader")
				.map((h) => h.textContent),
		).toEqual(["Channel and campaign", "Dates", "Budget", "Spent"]);
		expect(cells(rows[2] as HTMLElement)).toContain(
			`${NAME}Paused: the plan ended`,
		);
		expect(cells(rows[3] as HTMLElement)).toContain(
			"Bakery near mePaused at its budget",
		);
		// The newest pause first.
		expect(
			within(screen.getByRole("region", { name: "Ads Catervas paused" }))
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual([
			`Thu 12 Nov, 10:41${NAME}: the plan ended.`,
			"Thu 12 Nov, 10:15Bakery near me: it reached its budget, $150.00 of $150.00 USD.",
		]);
	});

	it("a_campaign_paused_twice_shows_its_newest_pause", async () => {
		await onThe12th({
			...RUNNING_PLAN,
			paused: [
				...RUNNING_PLAN.paused,
				{
					key: "near-me",
					name: "Bakery near me",
					why: "plan_ended",
					at: "2026-11-12T10:41:00Z",
				},
			],
		});
		await screen.findByRole("region", { name: "Spent so far" });
		expect(
			cells(within(budgetTable()).getAllByRole("row")[3] as HTMLElement),
		).toContain("Bakery near mePaused: the plan ended");
	});

	it("a_pause_for_google_ads_removed_is_told_as_that", async () => {
		await onThe12th({
			...RUNNING_PLAN,
			paused: [
				{
					key: "near-me",
					name: "Bakery near me",
					why: "connection_removed",
					at: "2026-11-12T10:50:00Z",
				},
			],
		});
		await screen.findByRole("region", { name: "Spent so far" });
		expect(
			cells(within(budgetTable()).getAllByRole("row")[3] as HTMLElement),
		).toContain("Bakery near mePaused: Google Ads was removed");
		expect(
			within(screen.getByRole("region", { name: "Ads Catervas paused" }))
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual(["Thu 12 Nov, 10:50Bakery near me: Google Ads was removed."]);
	});

	it("a_plan_with_no_ads_has_no_spend_section", async () => {
		await onThe12th({
			...RUNNING_PLAN,
			campaigns: [],
			budget: { total: "450.00", google_ads: "0.00" },
			spend: undefined,
			reached: [],
			paused: [],
		});
		await screen.findByRole("heading", { level: 1, name: TITLE });
		expect(screen.queryByRole("region", { name: "Spent so far" })).toBeNull();
		expect(
			screen.queryByRole("region", { name: "Ads Catervas paused" }),
		).toBeNull();
	});

	it("the_end_confirmation_says_the_ads_pause", async () => {
		const { s } = await onThe12th(RUNNING_PLAN);
		// The hint beside End says what ending does to the ads and the posts.
		expect(
			await screen.findByText(
				"Ending pauses the ads within a minute and stops the posts not yet sent.",
			),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "End the plan" }));
		const dialog = await screen.findByRole("dialog", {
			name: "End this plan now?",
		});
		// First thing: the ads pause, and what has been spent by Google's last figures.
		const [first] = within(dialog).getAllByText(/./, { selector: "p" });
		expect(first?.textContent).toBe(`${TITLE}, MP-3, on day 32 of 42.`);
		expect(dialog.textContent).toContain(
			"Catervas pauses its running ads within a minute. $318.65 is spent so far, by Google’s figures at 10:15.",
		);
		expect(s.calls("command")).toHaveLength(0);
	});

	it("the_end_confirmation_with_no_read_says_only_that_the_ads_pause", async () => {
		await onThe12th({ ...RUNNING_PLAN, spend: undefined });
		fireEvent.click(
			await screen.findByRole("button", { name: "End the plan" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "End this plan now?",
		});
		expect(dialog.textContent).toContain(
			"Catervas pauses its running ads within a minute.",
		);
		expect(dialog.textContent).not.toContain("is spent so far");
	});
});
