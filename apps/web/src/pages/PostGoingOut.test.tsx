import { expectNoAxeViolations } from "@catervas/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { GOING_OUT, todayWith } from "../test/posts.ts";
import { howSoon } from "./PostGoingOut.tsx";

/** The command the page sent, once it has sent `count`. */
const sent = (s: FakeSocket, count = 1) =>
	waitFor(() => {
		const c = s.calls("command")[count - 1];
		if (!c) throw new Error(`fewer than ${count} commands were sent`);
		return c;
	});

/** The row of the post going out whose words start with `words`. */
async function rowOf(words: string) {
	const rows = within(
		await screen.findByRole("list", { name: en.goingOutList }),
	).getAllByRole("listitem");
	const row = rows.find((one) => one.textContent?.includes(words));
	if (!row) throw new Error(`no row says "${words}"`);
	return row;
}

describe("a post going out", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("stop_asks_then_sends", async () => {
		const { container, s } = await todayWith({ posts: GOING_OUT });

		// A plan's post that Buffer does not have yet: its day in the plan is free again.
		const row = await rowOf("Thanksgiving pies are open");
		fireEvent.click(within(row).getByRole("button", { name: "Stop" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Stop this post?",
		});
		expect(within(dialog).getByText("Instagram, today at 13:00")).toBeTruthy();
		expect(
			within(dialog).getByText("Thanksgiving pies are open for pre-order."),
		).toBeTruthy();
		expect(within(dialog).getByText("It will not go out.")).toBeTruthy();
		expect(
			within(dialog).getByText(
				"Its day in your plan is free again, so Kai may write another post for it.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		// Closing sends nothing.
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);

		fireEvent.click(within(row).getByRole("button", { name: "Stop" }));
		const again = await screen.findByRole("dialog", {
			name: "Stop this post?",
		});
		fireEvent.click(
			within(again).getByRole("button", { name: "Stop the post" }),
		);
		const stop = await sent(s);
		expect(stop.params).toEqual({
			command: { command: "social_post_stop", body: { post: 42 } },
		});
		const asked = s
			.calls("query")
			.filter((frame) => frame.params.name === "social_posts.list").length;
		await s.reply(stop, { said: "stopped post 42", events: [9] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		// The list is asked for again, so that the post leaves it.
		await waitFor(() =>
			expect(
				s
					.calls("query")
					.filter((frame) => frame.params.name === "social_posts.list").length,
			).toBeGreaterThan(asked),
		);
	});

	it("says_what_stopping_takes_back_from_buffer", async () => {
		const { s } = await todayWith({ posts: GOING_OUT });

		// A post Buffer has: Catervas takes it back from Buffer, and no day of the plan is freed.
		const row = await rowOf("We open at 10 today");
		fireEvent.click(within(row).getByRole("button", { name: "Stop" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Stop this post?",
		});
		expect(within(dialog).getByText("It will not go out.")).toBeTruthy();
		expect(dialog.textContent).toContain(
			"Buffer already has it, so Catervas takes it back from Buffer.",
		);
		expect(dialog.textContent).not.toContain("free again");
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Stop the post" }),
		);
		expect((await sent(s)).params).toEqual({
			command: { command: "social_post_stop", body: { post: 41 } },
		});
	});

	it("a_post_you_allowed_frees_no_day_of_the_plan", async () => {
		// Post 43 was one the owner allowed: it is in no plan.
		const posts = {
			posts: GOING_OUT.posts.map((post) =>
				post.post === 43
					? { ...post, plan: undefined, slot: undefined, approved_by: "owner" }
					: post,
			),
		};
		await todayWith({ posts });

		const row = await rowOf("Pie pre-orders are open");
		expect(row.textContent).toContain(
			"You allowed this. Catervas hands it to Buffer",
		);
		fireEvent.click(within(row).getByRole("button", { name: "Stop" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Stop this post?",
		});
		expect(within(dialog).getByText("It will not go out.")).toBeTruthy();
		expect(dialog.textContent).not.toContain("free again");
		expect(dialog.textContent).not.toContain("takes it back from Buffer");
	});

	it("a_stop_buffer_refuses_says_what_to_do", async () => {
		const { container, s } = await todayWith({ posts: GOING_OUT });
		const row = await rowOf("We open at 10 today");
		fireEvent.click(within(row).getByRole("button", { name: "Stop" }));
		const dialog = await screen.findByRole("dialog", {
			name: "Stop this post?",
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Stop the post" }),
		);

		await s.reply(await sent(s), {
			error: {
				kind: "refused",
				detail:
					"post_not_taken_back: Buffer did not take it back; delete it in Buffer before Mon 26 Oct 10:00",
			},
		});

		// What to do, in the post's own time, and where; the daemon's words never show.
		const alert = await within(dialog).findByRole("alert");
		expect(alert.textContent).toBe(
			"Buffer did not take it back. Delete it in Buffer before 10:00, or it goes out.",
		);
		const open = within(dialog).getByRole("link", { name: "Open Buffer" });
		expect(open.getAttribute("href")).toBe("https://publish.buffer.com");
		expect(open.getAttribute("target")).toBe("_blank");
		expect(open.getAttribute("rel")).toContain("noopener");
		expect(open.getAttribute("rel")).toContain("noreferrer");
		// The post is still going out.
		expect(screen.getByRole("heading", { name: "Going out (4)" })).toBeTruthy();
		expect(await rowOf("We open at 10 today")).toBeTruthy();
		await expectNoAxeViolations(container);

		// Catervas is handing a post over this moment: the owner is told to wait a minute.
		const second = await screen.findByRole("dialog", {
			name: "Stop this post?",
		});
		fireEvent.click(
			within(second).getByRole("button", { name: "Stop the post" }),
		);
		await s.reply(await sent(s, 2), {
			error: {
				kind: "refused",
				detail:
					"post_being_handed_over: Catervas is giving it to Buffer now; stop it again in a minute",
			},
		});
		await waitFor(() =>
			expect(within(second).getByRole("alert").textContent).toBe(
				"Catervas is giving it to Buffer now. Stop it again in a minute.",
			),
		);
		expect(
			within(second).queryByRole("link", { name: "Open Buffer" }),
		).toBeNull();
	});
});

describe("how soon a post goes out", () => {
	const now = new Date(2026, 9, 26, 9, 15);
	const later = (days: number, hours: number, minutes: number, seconds = 0) =>
		new Date(
			now.getTime() +
				(((days * 24 + hours) * 60 + minutes) * 60 + seconds) * 1000,
		);

	it("says_how_soon_in_minutes_hours_and_days", () => {
		expect(howSoon(later(0, 0, 1), now)).toBe("in 1 minute");
		expect(howSoon(later(0, 0, 45), now)).toBe("in 45 minutes");
		// A part of a minute is a minute: a post is never said to go out sooner than it does.
		expect(howSoon(later(0, 0, 44, 30), now)).toBe("in 45 minutes");
		expect(howSoon(later(0, 1, 0), now)).toBe("in 1 hour");
		expect(howSoon(later(0, 1, 5), now)).toBe("in 1 hour 5 minutes");
		expect(howSoon(later(0, 3, 45), now)).toBe("in 3 hours 45 minutes");
		expect(howSoon(later(0, 3, 0), now)).toBe("in 3 hours");
		expect(howSoon(later(0, 23, 59), now)).toBe("in 23 hours 59 minutes");
		// From a day on, in days of the calendar: Wednesday morning from Monday morning.
		expect(howSoon(new Date(2026, 9, 27, 10, 0), now)).toBe("in 1 day");
		expect(howSoon(new Date(2026, 9, 28, 8, 30), now)).toBe("in 2 days");
		expect(howSoon(new Date(2026, 9, 31, 9, 0), now)).toBe("in 5 days");
		// Late is no time at all.
		expect(howSoon(later(0, 0, -3), now)).toBe("in 1 minute");
	});
});
