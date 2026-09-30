import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { media } from "../test/media.ts";
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
		agent("sol", "Sol", "scrum_master", "scrum-master"),
		agent("theo", "Theo", "software_developer", "developer"),
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
	fields: { waiting?: unknown[]; sprint?: unknown; meetings?: unknown[] } = {},
) {
	const { container, socket } = await renderApp("/channel");
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

		// An agent's post: its avatar, name, role tag and time.
		const theo = row("Started on the menu page.");
		expect(within(theo).getByRole("img").getAttribute("alt")).toBe("Theo");
		expect(within(theo).getByText("Theo", { selector: "strong" })).toBeTruthy();
		expect(within(theo).getByTitle("Developer")).toBeTruthy();
		expect(within(theo).getByText("09:02")).toBeTruthy();

		// Farik's line: no avatar.
		const system = row("Sprint 2 started.");
		expect(within(system).queryByRole("img")).toBeNull();
		expect(within(system).getByText(en.brand)).toBeTruthy();

		// Your own post, labelled You, and a reply to it.
		const you = row("Thanks, all.");
		expect(within(you).getByText(en.channelYou, { selector: "strong" }));
		expect(within(you).queryByRole("img")).toBeNull();
		expect(within(row("Glad to.")).getByText("Replying to You")).toBeTruthy();
		await expectNoAxeViolations(container);
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
		expect(options.map((o) => o.textContent)).toEqual(["Mira", "Sol", "Theo"]);
		await expectNoAxeViolations(container);
		fireEvent.keyDown(box, { key: "ArrowDown" });
		fireEvent.keyDown(box, { key: "ArrowDown" });
		expect(box.getAttribute("aria-activedescendant")).toBe(
			options[2]?.getAttribute("id"),
		);
		fireEvent.keyDown(box, { key: "Enter" });
		expect(box.value).toBe("@theo ");
		expect(screen.queryByRole("listbox")).toBeNull();

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
		act(() => s.reply(sent, { said: "posted", events: [5] }));
		await waitFor(() => expect(box.value).toBe(""));

		// The length is counted in characters, as Farik counts it: 2000 emoji pass.
		fireEvent.change(box, { target: { value: "😀".repeat(2000) } });
		fireEvent.click(screen.getByRole("button", { name: en.channelPost }));
		await waitFor(() => expect(s.calls("command")).toHaveLength(2));
		act(() =>
			s.reply(s.calls("command")[1] as never, {
				error: { kind: "refused", detail: "paused: the team is paused" },
			}),
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			"The team is paused",
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
		act(() =>
			s.emit("message", {
				data: JSON.stringify({
					jsonrpc: "2.0",
					method: "event",
					params: {
						event: {
							seq: 90,
							recorded_at: "2026-09-28T09:03:00Z",
							team_id: "t",
							project_id: "p",
							agent_id: "theo",
							kind: "message.posted",
							body: {
								author: "theo",
								kind: "reply",
								text: "On it.",
								mentions: [],
								in_reply_to: first,
							},
						},
					},
				}),
			}),
		);
		const row = (await screen.findByText("On it.")).closest(
			"li",
		) as HTMLElement;
		expect(within(row).getByText("Replying to You")).toBeTruthy();
		expect(
			s.calls("query").filter((f) => f.params.name === "channel.messages"),
		).toHaveLength(1);
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
