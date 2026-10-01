import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { media } from "../test/media.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
import styles from "./Channel.module.css";

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
		agent("sol", "Sol", "scrum_master", "scrum-master"),
		agent("theo", "Theo", "software_developer", "developer"),
		{ ...agent("ada", "Ada", "architect", "architect"), status: "retired" },
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};

let seq = 0;
/** A message as `channel.messages` answers it. */
const message = (
	author: string,
	kind: string,
	text: string,
	more: Record<string, unknown> = {},
) => ({
	seq: ++seq,
	at: "2026-09-28T09:02:00Z",
	author,
	kind,
	text,
	mentions: [],
	thread: null,
	in_reply_to: null,
	task_id: null,
	...more,
});

/** The channel page, with the team, the messages and each list answered. */
async function channel(
	messages: unknown[],
	fields: {
		waiting?: unknown[];
		sprint?: unknown;
		meetings?: unknown[];
		path?: string;
	} = {},
) {
	const { container, socket } = await renderApp(fields.path ?? "/channel");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "channel.messages", { messages });
	await answerQuery(s, "waiting.list", { waiting: fields.waiting ?? [] });
	await answerQuery(s, "sprint.current", fields.sprint ?? null);
	if (fields.sprint)
		await answerQuery(s, "sprint.get", {
			sprint_id: "S2",
			status: "open",
			started_at: "2026-09-21T09:00:00Z",
			spent_usd: 0,
			task_count: 0,
			done_count: 0,
			tasks: [],
			meetings: fields.meetings ?? [],
		});
	return { container, s };
}

/** One recorded event, as the daemon's subscription sends it. */
const play = (s: FakeSocket, seq: number, kind: string, body: object = {}) =>
	s.emit("message", {
		data: JSON.stringify({
			jsonrpc: "2.0",
			method: "event",
			params: {
				event: {
					seq,
					recorded_at: "2026-09-28T09:03:00Z",
					team_id: "t",
					project_id: "p",
					agent_id: "theo",
					kind,
					body,
				},
			},
		}),
	});

/** Plays one recorded event to the page. */
const live = (s: FakeSocket, seq: number, kind: string, body: object = {}) =>
	act(() => play(s, seq, kind, body));

/** Each `channel.messages` query the page asked, in order. */
const pages = (s: FakeSocket) =>
	s.calls("query").filter((f) => f.params.name === "channel.messages");

const day = (offset: number) =>
	new Date(Date.now() + offset * 86_400_000).toISOString().slice(0, 10);
const weekday = (date: string) =>
	new Date(`${date}T12:00:00Z`).toLocaleDateString("en-GB", {
		weekday: "long",
		timeZone: "UTC",
	});

describe("channel", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_each_kind_of_message", async () => {
		const { container } = await channel([
			message("theo", "reaction", "Started on the menu page."),
			message("farik", "system", "Sprint 2 started."),
			message("human", "human", "Thanks, all."),
			message("theo", "reply", "Glad to.", { in_reply_to: seq }),
		]);
		expect(
			await screen.findByRole("heading", { level: 1, name: en.channel }),
		).toBeTruthy();
		expect(screen.getByText(en.channelIntro)).toBeTruthy();
		const list = screen.getByRole("list", { name: en.channelMessages });
		const row = (text: string) =>
			within(list).getByText(text).closest("li") as HTMLElement;
		// Oldest first, as the page came.
		expect(
			within(list)
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual([
			expect.stringContaining("Started on the menu page."),
			expect.stringContaining("Sprint 2 started."),
			expect.stringContaining("Thanks, all."),
			expect.stringContaining("Glad to."),
		]);

		// An agent's post: its avatar, name, role tag and time.
		const theo = row("Started on the menu page.");
		expect(within(theo).getByRole("img").getAttribute("alt")).toBe("Theo");
		expect(within(theo).getByText("Theo", { selector: "strong" })).toBeTruthy();
		expect(within(theo).getByTitle("Developer")).toBeTruthy();
		expect(within(theo).getByText("09:02")).toBeTruthy();

		// Farik's line: small and muted, with no avatar and no role.
		const system = row("Sprint 2 started.");
		expect(system.className).toBe(styles.system);
		expect(within(system).queryByRole("img")).toBeNull();
		expect(within(system).getByText(en.brand)).toBeTruthy();

		// Your own post, labelled You, and a reply to it.
		const you = row("Thanks, all.");
		expect(within(you).getByText(en.channelYou, { selector: "strong" }));
		expect(within(you).queryByRole("img")).toBeNull();
		expect(within(row("Glad to.")).getByText("Replying to You")).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("words_farik_move_lines_as_the_history_tab_does", async () => {
		await channel([
			message("farik", "system", "FRK-1 refining → ready (by the governor)"),
			message(
				"farik",
				"system",
				"FRK-2 verifying → rejected (by theo): C1 failed: <b>no</b> file",
			),
			message(
				"farik",
				"system",
				"FRK-3 escalated → in_progress (by the human): go on",
			),
			message("farik", "system", "FRK-4 dreaming → flying (by theo)"),
		]);
		const list = await screen.findByRole("list", { name: en.channelMessages });
		const rows = within(list).getAllByRole("listitem");
		expect(rows.map((li) => li.textContent?.replace(/\d\d:\d\d$/, ""))).toEqual(
			[
				"Farik moved FRK-1 to To do. ",
				"Theo sent FRK-2 back. Why: C1 failed: <b>no</b> file ",
				"You moved FRK-3 to In progress. Why: go on ",
				// A line of another shape stays as Farik wrote it.
				"Farik FRK-4 dreaming → flying (by theo) ",
			],
		);
		// The task is a link to it; the agent's words stay words, never markup.
		expect(
			within(rows[0] as HTMLElement)
				.getByRole("link", { name: "FRK-1" })
				.getAttribute("href"),
		).toBe("/tasks/FRK-1");
		expect(list.querySelector("b")).toBeNull();
	});

	it("groups_ceremonies_into_threads", async () => {
		const [yesterday, today] = [day(-1), day(0)];
		const at = (date: string, time: string) => `${date}T${time}:00Z`;
		const { container } = await channel(
			[
				message("sol", "ceremony", "Here is the plan.", {
					thread: "planning",
					at: at(yesterday, "09:00"),
				}),
				message("mira", "ceremony", "Looks right.", {
					thread: "planning",
					at: at(yesterday, "09:05"),
				}),
				message("sol", "ceremony", "Yesterday's standup.", {
					thread: "standup",
					at: at(yesterday, "10:00"),
				}),
				message("theo", "ambient", "Coffee first.", {
					at: at(today, "08:00"),
				}),
				message("sol", "ceremony", "Today's standup.", {
					thread: "standup",
					at: at(today, "09:00"),
				}),
				message("theo", "ceremony", "Menu page today.", {
					thread: "standup",
					at: at(today, "09:01"),
				}),
				message("sol", "ceremony", "Thanks.", {
					thread: "standup",
					at: at(today, "09:02"),
				}),
			],
			{
				sprint: { sprint_id: "S2", done: 0, total: 1 },
				meetings: [
					{
						thread: "planning",
						first_seq: 1,
						at: at(yesterday, "09:00"),
						posts: 2,
					},
				],
			},
		);
		const list = await screen.findByRole("list", { name: en.channelMessages });
		const toggle = (name: string) =>
			within(list).getByRole("button", { name: new RegExp(`^${name}`) });
		const planning = toggle(`Planning, ${weekday(yesterday)}: 2 people posted`);
		const oldStandup = toggle(
			`Standup, ${weekday(yesterday)}: 1 person posted`,
		);
		const standup = toggle(`Standup, ${weekday(today)}: 2 people posted`);
		expect(within(list).getAllByRole("button")).toHaveLength(3);

		// Only today's standup starts open.
		expect(planning.getAttribute("aria-expanded")).toBe("false");
		expect(oldStandup.getAttribute("aria-expanded")).toBe("false");
		expect(standup.getAttribute("aria-expanded")).toBe("true");
		expect(screen.queryByText("Here is the plan.")).toBeNull();
		expect(screen.getByText("Menu page today.")).toBeTruthy();
		expect(screen.getByText("Coffee first.")).toBeTruthy();
		fireEvent.click(planning);
		expect(planning.getAttribute("aria-expanded")).toBe("true");
		expect(screen.getByText("Here is the plan.")).toBeTruthy();

		// Each block has an anchor, and the side panel links to the sprint's meetings.
		expect(container.querySelector(`#thread-standup-${today}`)).toBeTruthy();
		const side = screen.getByRole("complementary", {
			name: en.channelMeetings,
		});
		expect(
			within(side)
				.getByRole("link", { name: `Planning, ${weekday(yesterday)}` })
				.getAttribute("href"),
		).toBe(`#thread-planning-${yesterday}`);
		await expectNoAxeViolations(container);
	});

	it("links_tasks_and_mentions", async () => {
		const { container } = await channel(
			[
				message(
					"theo",
					"reaction",
					"@human FRK-2 is ready, and @mira has the plan. <img src=x onerror=alert(1)>",
					{ task_id: "FRK-3", mentions: ["mira"] },
				),
			],
			{
				waiting: [
					{
						task_id: "FRK-2",
						kind: "acceptance",
						agent_id: "theo",
						title: "Menu page",
						line: "Ready",
					},
				],
			},
		);
		const list = await screen.findByRole("list", { name: en.channelMessages });
		const link = within(list).getByRole("link", { name: "FRK-2" });
		expect(link.getAttribute("href")).toBe("/tasks/FRK-2");
		expect(link.nextSibling?.textContent).toContain(en.channelWaitingOnYou);
		expect(
			within(list).getByRole("link", { name: "FRK-3" }).getAttribute("href"),
		).toBe("/tasks/FRK-3");
		expect(within(list).getByText(en.channelAtYou)).toBeTruthy();
		expect(within(list).getByText("@Mira")).toBeTruthy();

		// Agent text is untrusted: markup in it is shown as text, never made.
		expect(container.querySelector("img[onerror]")).toBeNull();
		expect(list.textContent).toContain("<img src=x onerror=alert(1)>");
		await expectNoAxeViolations(container);
	});

	it("links_a_task_whose_id_starts_a_longer_one", async () => {
		await channel([
			message("theo", "reaction", "FRK-10 is done.", { task_id: "FRK-1" }),
		]);
		const list = await screen.findByRole("list", { name: en.channelMessages });
		expect(within(list).getByRole("link", { name: "FRK-10" })).toBeTruthy();
		expect(within(list).getByRole("link", { name: "FRK-1" })).toBeTruthy();
	});

	it("posts_and_mentions", async () => {
		media.set("(min-width: 1024px)", true);
		const { container, s } = await channel([]);
		const box = (await screen.findByRole("textbox", {
			name: en.channelPostLabel,
		})) as HTMLTextAreaElement;
		expect(screen.getByText(en.channelHintLink).closest("p")?.textContent).toBe(
			"A mentioned agent answers here. To have work done, send a request instead.",
		);

		// `@` opens the team, and arrows and Enter pick one.
		fireEvent.change(box, { target: { value: "@" } });
		const options = within(
			screen.getByRole("listbox", { name: en.channelMentionList }),
		).getAllByRole("option");
		// A screen reader hears that the list opened.
		expect(
			screen
				.getByText("People to mention: 3. Use the arrow keys, then Enter.")
				.closest("[aria-live=polite]"),
		).toBeTruthy();
		// A retired agent is not offered.
		expect(options.map((o) => o.textContent)).toEqual(["Mira", "Sol", "Theo"]);
		await expectNoAxeViolations(container);
		const active = () =>
			options.findIndex(
				(o) => o.id === box.getAttribute("aria-activedescendant"),
			);
		expect(active()).toBe(0);
		// The arrows wrap at both ends.
		fireEvent.keyDown(box, { key: "ArrowUp" });
		expect(active()).toBe(2);
		fireEvent.keyDown(box, { key: "ArrowDown" });
		expect(active()).toBe(0);
		fireEvent.keyDown(box, { key: "ArrowUp" });
		fireEvent.keyDown(box, { key: "ArrowUp" });
		expect(active()).toBe(1);
		fireEvent.keyDown(box, { key: "ArrowDown" });
		expect(active()).toBe(2);
		fireEvent.keyDown(box, { key: "Enter" });
		expect(box.value).toBe("@theo ");
		expect(screen.queryByRole("listbox")).toBeNull();

		// Escape closes the list and leaves the words as typed.
		fireEvent.change(box, { target: { value: "@m" } });
		expect(screen.getByRole("listbox")).toBeTruthy();
		fireEvent.keyDown(box, { key: "Escape" });
		expect(screen.queryByRole("listbox")).toBeNull();
		expect(box.value).toBe("@m");

		// Post sends the words.
		const text = "@theo can you look at the menu page?";
		fireEvent.change(box, { target: { value: text } });
		fireEvent.click(screen.getByRole("button", { name: en.channelPost }));
		const sent = await waitFor(() => {
			const f = s.calls("command")[0];
			if (!f) throw new Error("no command was sent");
			return f;
		});
		expect(sent.params).toEqual({
			command: { command: "message_post", body: { text } },
		});
		await s.reply(sent, { said: "posted", events: [5] });
		await waitFor(() => expect(box.value).toBe(""));

		// The length is counted in characters, as Farik counts it: 2000 emoji pass.
		fireEvent.change(box, { target: { value: "😀".repeat(2000) } });
		fireEvent.click(screen.getByRole("button", { name: en.channelPost }));
		await waitFor(() => expect(s.calls("command")).toHaveLength(2));
		await s.reply(s.calls("command")[1] as never, {
			error: { kind: "invalid", detail: "a message is blank" },
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseCommand,
		);

		// Too long a text is refused before sending.
		fireEvent.change(box, { target: { value: "😀".repeat(2001) } });
		fireEvent.click(screen.getByRole("button", { name: en.channelPost }));
		expect((await screen.findByRole("alert")).textContent).toBe(
			"A message is at most 2000 characters, and this one is 2001.",
		);
		expect(s.calls("command")).toHaveLength(2);
	});

	it("appends_new_messages_live", async () => {
		const { s } = await channel([
			message("human", "human", "@theo can you look?", { mentions: ["theo"] }),
		]);
		const first = seq;
		await screen.findByText("can you look?", { exact: false });
		live(s, 90, "message.posted", {
			author: "theo",
			kind: "reply",
			text: "On it.",
			mentions: [],
			in_reply_to: first,
		});
		const row = (await screen.findByText("On it.")).closest(
			"li",
		) as HTMLElement;
		expect(within(row).getByText("Replying to You")).toBeTruthy();

		// One heard out of order still takes its place by seq.
		live(s, 89, "message.posted", {
			author: "mira",
			kind: "ambient",
			text: "Before that.",
			mentions: [],
		});
		await screen.findByText("Before that.");
		const list = screen.getByRole("list", { name: en.channelMessages });
		expect(
			within(list)
				.getAllByRole("listitem")
				.map((li) => li.textContent),
		).toEqual([
			expect.stringContaining("can you look?"),
			expect.stringContaining("Before that."),
			expect.stringContaining("On it."),
		]);
		expect(
			s.calls("query").filter((f) => f.params.name === "channel.messages"),
		).toHaveLength(1);
	});

	it("keeps_a_live_message_after_500_other_events", async () => {
		const { s } = await channel([message("human", "human", "Morning.")]);
		await screen.findByText("Morning.");
		live(s, 1000, "message.posted", {
			author: "theo",
			kind: "reaction",
			text: "Started.",
			mentions: [],
		});
		await screen.findByText("Started.");
		act(() => {
			for (let n = 1001; n <= 1501; n++) play(s, n, "task.created");
		});
		expect(screen.getByText("Started.")).toBeTruthy();
	});

	it("adds_a_live_message_once_and_settles", async () => {
		const { s } = await channel([message("human", "human", "Morning.")]);
		await screen.findByText("Morning.");
		const said = vi.spyOn(console, "error").mockImplementation(() => {});
		// Played outside act: a fold that re-adds what it holds would update itself for ever, and
		// inside act that loop hangs the test with no failure; here it shows as the message twice.
		play(s, 1000, "message.posted", {
			author: "theo",
			kind: "reaction",
			text: "Started.",
			mentions: [],
		});
		await screen.findByText("Started.");
		// Load can only let fewer passes run in this wait, so it can only pass falsely, never fail.
		await new Promise((r) => setTimeout(r, 50));
		expect(screen.getAllByText("Started.")).toHaveLength(1);
		expect(said).not.toHaveBeenCalled();
		said.mockRestore();
	});

	it("shows_a_message_once_when_its_page_and_its_event_both_hold_it", async () => {
		const { s } = await channel([
			message("theo", "reaction", "Only once.", { seq: 5 }),
		]);
		await screen.findByText("Only once.");
		live(s, 5, "message.posted", {
			author: "theo",
			kind: "reaction",
			text: "Only once.",
			mentions: [],
		});
		expect(screen.getAllByText("Only once.")).toHaveLength(1);
	});

	it("shows_earlier_messages", async () => {
		const page = Array.from({ length: 100 }, (_, i) =>
			message("theo", "ambient", `Newer ${i}.`, { seq: 1001 + i }),
		);
		const { s } = await channel(page);
		const more = await screen.findByRole("button", { name: en.channelEarlier });
		fireEvent.click(more);
		const asked = await waitFor(() => {
			const f = pages(s)[1];
			if (!f) throw new Error("no second page was asked");
			return f;
		});
		expect(asked.params.params).toEqual({ before_seq: 1001, limit: 100 });
		// One page at a time.
		expect((more as HTMLButtonElement).disabled).toBe(true);
		await s.reply(asked, {
			messages: [998, 999, 1000].map((n) =>
				message("mira", "ambient", `Older ${n}.`, { seq: n }),
			),
		});
		await screen.findByText("Older 998.");
		const list = screen.getByRole("list", { name: en.channelMessages });
		expect(
			within(list)
				.getAllByRole("listitem")
				.slice(0, 4)
				.map((li) => li.textContent),
		).toEqual([
			expect.stringContaining("Older 998."),
			expect.stringContaining("Older 999."),
			expect.stringContaining("Older 1000."),
			expect.stringContaining("Newer 0."),
		]);
		// Fewer than a page: there is nothing earlier.
		expect(
			screen.queryByRole("button", { name: en.channelEarlier }),
		).toBeNull();
	});

	it("says_when_earlier_messages_cannot_be_read", async () => {
		const page = Array.from({ length: 100 }, (_, i) =>
			message("theo", "ambient", `Newer ${i}.`, { seq: 1001 + i }),
		);
		const { s } = await channel(page);
		fireEvent.click(
			await screen.findByRole("button", { name: en.channelEarlier }),
		);
		const asked = await waitFor(() => {
			const f = pages(s)[1];
			if (!f) throw new Error("no second page was asked");
			return f;
		});
		await s.fail(asked, -32000, "the store is busy");
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.channelEarlierFailed,
		);
		expect(
			screen.getByRole("button", { name: en.channelEarlier }),
		).toBeTruthy();
	});

	describe("a meeting link", () => {
		const old = day(-3);
		const today = `${day(0)}T09:00:00Z`;
		const anchor = `thread-planning-${old}`;
		const full = () =>
			Array.from({ length: 100 }, (_, i) =>
				message("theo", "ambient", `Newer ${i}.`, { seq: 1001 + i, at: today }),
			);
		const planning = [1, 2].map((n) =>
			message("sol", "ceremony", `Plan ${n}.`, {
				seq: n,
				thread: "planning",
				at: `${old}T09:0${n}:00Z`,
			}),
		);
		const scroll = vi.fn();
		beforeEach(() => {
			scroll.mockClear();
			Element.prototype.scrollIntoView = scroll;
		});
		afterEach(() => {
			delete (Element.prototype as Partial<Element>).scrollIntoView;
		});
		/** Answers the page before the loaded ones with `messages`. */
		const answerEarlier = async (s: FakeSocket, messages: unknown[]) => {
			const asked = await waitFor(() => {
				const f = pages(s)[1];
				if (!f) throw new Error("no earlier page was asked");
				return f;
			});
			expect(asked.params.params).toEqual({ before_seq: 1001, limit: 100 });
			await s.reply(asked, { messages });
		};
		const opened = async () => {
			const block = await screen.findByRole("button", {
				name: new RegExp(`^Planning, ${weekday(old)}`),
			});
			await waitFor(() =>
				expect(block.getAttribute("aria-expanded")).toBe("true"),
			);
			expect(screen.getByText("Plan 1.")).toBeTruthy();
			await waitFor(() => expect(scroll).toHaveBeenCalled());
			expect(scroll.mock.contexts.at(-1)).toBe(document.getElementById(anchor));
		};

		it("pages_back_to_a_thread_older_than_the_loaded_page", async () => {
			const { s } = await channel(full(), { path: `/channel#${anchor}` });
			await answerEarlier(s, planning);
			await opened();
		});

		it("opens_a_thread_from_the_side_panel", async () => {
			const { s } = await channel(full(), {
				sprint: { sprint_id: "S2", done: 0, total: 1 },
				meetings: [
					{
						thread: "planning",
						first_seq: 1,
						at: `${old}T09:01:00Z`,
						posts: 2,
					},
				],
			});
			const side = await screen.findByRole("complementary", {
				name: en.channelMeetings,
			});
			fireEvent.click(
				await within(side).findByRole("link", {
					name: `Planning, ${weekday(old)}`,
				}),
			);
			await answerEarlier(s, planning);
			await opened();
		});

		it("stops_paging_once_the_pages_are_older_than_its_day", async () => {
			const before = full().map((m) => ({ ...m, at: `${day(-5)}T09:00:00Z` }));
			const { s } = await channel(before, { path: `/channel#${anchor}` });
			expect((await screen.findByRole("status")).textContent).toBe(
				en.channelMeetingGone,
			);
			expect(pages(s)).toHaveLength(1);
		});

		it("says_when_the_thread_is_no_longer_in_the_channel", async () => {
			await channel([message("theo", "ambient", "Only this.", { at: today })], {
				path: `/channel#${anchor}`,
			});
			expect((await screen.findByRole("status")).textContent).toBe(
				en.channelMeetingGone,
			);
		});
	});

	it("previews_the_channel_on_today", async () => {
		const { container, socket } = await renderApp("/");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", { team: TEAM });
		await answerQuery(s, "channel.messages", {
			messages: [
				message("theo", "reaction", "An older one."),
				message("mira", "ambient", "The plan is nearly done."),
				message("human", "human", "Thanks, Mira."),
				message("farik", "system", "Sprint 2 started."),
			],
		});
		const preview = await screen.findByRole("region", {
			name: en.channelPreview,
		});
		expect(within(preview).getByText("The plan is nearly done.")).toBeTruthy();
		expect(within(preview).getByText("Mira")).toBeTruthy();
		expect(within(preview).getByText("Thanks, Mira.")).toBeTruthy();
		expect(within(preview).getByText(en.channelYou)).toBeTruthy();
		expect(within(preview).queryByText("An older one.")).toBeNull();
		expect(within(preview).queryByText("Sprint 2 started.")).toBeNull();
		expect(
			within(preview)
				.getByRole("link", { name: en.channelOpen })
				.getAttribute("href"),
		).toBe("/channel");
		await expectNoAxeViolations(container);
	});
});
