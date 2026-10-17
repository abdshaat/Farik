import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import shell from "../shell/Shell.module.css";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { sentCommand } from "../test/gate.ts";
import { showsWhatItHides } from "../test/hidden.ts";
import { media } from "../test/media.ts";
import { APPROVED, HEARD, INTERVIEWING } from "../test/productPlan.ts";
import {
	answerQuery,
	answerStatus,
	eventArrives,
	renderApp,
} from "../test/render-app.tsx";
import { bodyOf, refusedBy } from "../test/schema.ts";
import channel from "./Channel.module.css";
import own from "./Chats.module.css";

const WIDE = "(min-width: 1024px)";
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
		{
			...agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
			status: "retired",
		},
		agent("theo", "Theo", "software_developer", "developer"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};
const line = (seq: number, author: string, text: string) => ({
	seq,
	at: "2026-09-28T09:44:00Z",
	author,
	text,
});
const LIST = {
	team_last: line(3, "theo", "Finished another session."),
	chats: [
		{ agent_id: "mira", retired: false, last: line(5, "mira", "Yes. It can.") },
		{ agent_id: "sol", retired: false, last: line(4, "human", "Can it wait?") },
		{ agent_id: "kai", retired: true, last: line(2, "kai", "Bye.") },
		{ agent_id: "theo", retired: false, last: null },
	],
};

let seq = 0;
/** A message as `chat.messages` answers it. */
const chat = (
	author: string,
	text: string,
	more: Record<string, unknown> = {},
) => ({
	seq: ++seq,
	at: "2026-09-28T09:40:00Z",
	author,
	text,
	in_reply_to: null,
	request: null,
	sent_as: null,
	...more,
});

/** The chats at `path`, with the team, the list and the open chat answered. */
async function chats(
	path: string,
	fields: {
		messages?: unknown[];
		waiting?: unknown;
		list?: unknown;
		plan?: unknown;
	} = {},
) {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "chats.list", fields.list ?? LIST);
	if (path === "/channel") {
		await answerQuery(s, "channel.messages", { messages: [] });
		await answerQuery(s, "waiting.list", { waiting: [] });
	} else if (path !== "/channel/nobody")
		await answerQuery(s, "chat.messages", {
			messages: fields.messages ?? [],
			waiting: fields.waiting ?? null,
		});
	if (fields.plan) await answerQuery(s, "product_plan.state", fields.plan);
	return { container, s };
}

/** Plays one recorded event to the page, by `agent`. */
const live = (
	s: FakeSocket,
	seq: number,
	kind: string,
	body: object,
	envelope: object = {},
) =>
	act(() =>
		s.emit("message", {
			data: JSON.stringify({
				jsonrpc: "2.0",
				method: "event",
				params: {
					event: {
						seq,
						recorded_at: "2026-09-28T09:45:00Z",
						team_id: "t",
						project_id: "p",
						agent_id: "mira",
						kind,
						body,
						...envelope,
					},
				},
			}),
		}),
	);

const asked = (s: FakeSocket) =>
	s.calls("query").filter((f) => f.params.name === "chat.messages");

const APPLE = {
	title: "Let customers pay with Apple Pay",
	text: "Add Apple Pay to the checkout page, next to paying by card.",
};

/** The box's size, which jsdom does not lay out: the tests set what the browser would say. */
function stubBox(scrollHeight = 2000, clientHeight = 500) {
	for (const [key, value] of Object.entries({ scrollHeight, clientHeight }))
		Object.defineProperty(HTMLElement.prototype, key, {
			configurable: true,
			get: () => value,
		});
}

/** A full page of `chat.messages`, seq 201 to 300, Mira's. */
const fullPage = () =>
	Array.from({ length: 100 }, (_, i) =>
		chat("mira", `Newer ${i}.`, { seq: 201 + i }),
	);

describe("chats", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		for (const key of ["scrollHeight", "clientHeight"])
			Reflect.deleteProperty(HTMLElement.prototype, key);
	});

	it("lists_the_chats", async () => {
		media.set(WIDE, true);
		await chats("/channel");
		const rail = screen.getByRole("navigation", { name: en.navRail });
		expect(within(rail).getByRole("link", { name: en.chats })).toBeTruthy();
		expect(within(rail).queryByText("Channel")).toBeNull();

		const list = await screen.findByRole("navigation", { name: en.chats });
		const links = within(list).getAllByRole("link");
		expect(links.map((a) => a.getAttribute("href"))).toEqual([
			"/channel",
			"/channel/mira",
			"/channel/sol",
			"/channel/theo",
			"/channel/kai",
		]);
		// Team first, open; each agent with its avatar, name, role and last line.
		expect(links[0]?.getAttribute("aria-current")).toBe("page");
		expect(links[0]?.textContent).toContain(en.chatsEveryone);
		expect(links[0]?.textContent).toContain("Theo: Finished another session.");
		const mira = links[1] as HTMLElement;
		expect(within(mira).getByRole("img").getAttribute("alt")).toBe("Mira");
		expect(within(mira).getByText("Mira")).toBeTruthy();
		expect(within(mira).getByText("Product Manager")).toBeTruthy();
		expect(within(mira).getByText("Mira: Yes. It can.")).toBeTruthy();
		expect(links[2]?.textContent).toContain("You: Can it wait?");
		expect(links[3]?.textContent).toContain(
			"No messages yet. Ask Theo anything.",
		);
		// The retired one, under "Past teammates".
		const past = within(list).getByRole("heading", { name: en.chatsPast });
		expect(
			past.compareDocumentPosition(links[4] as HTMLElement) &
				Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		expect(links[4]?.textContent).toContain(
			"Kai has left the team. Kept to read.",
		);
	});

	it("fills_the_window_on_the_chats", async () => {
		const rootOf = async () =>
			(await screen.findByRole("main")).parentElement as HTMLElement;
		for (const path of ["/channel", "/channel/mira"]) {
			await chats(path);
			expect((await rootOf()).classList).toContain(shell.fill);
			cleanup();
		}
		// Another path that only begins with the same letters is not the chats.
		for (const path of ["/board", "/channelx"]) {
			const { socket } = await renderApp(path);
			await answerStatus(socket as FakeSocket, false);
			expect((await rootOf()).classList).not.toContain(shell.fill);
			cleanup();
		}
	});

	it("opens_a_one_to_one", async () => {
		media.set(WIDE, true);
		const { s } = await chats("/channel/mira", {
			messages: [
				chat("human", "Two lines:\nthe second."),
				chat("mira", "Yes.", { in_reply_to: seq }),
			],
		});
		expect(asked(s).at(0)?.params.params).toEqual({
			agent_id: "mira",
			limit: 100,
		});
		expect(
			await screen.findByRole("heading", {
				level: 1,
				name: "Talking with Mira",
			}),
		).toBeTruthy();
		expect(
			screen.getByRole("link", { name: en.chatsBack }).getAttribute("href"),
		).toBe("/channel");
		const list = screen.getByRole("navigation", { name: en.chats });
		expect(
			within(list).getByRole("link", { current: "page" }).getAttribute("href"),
		).toBe("/channel/mira");

		const messages = screen.getByRole("list", { name: en.channelMessages });
		const rows = within(messages).getAllByRole("listitem");
		expect(rows).toHaveLength(2);
		// Yours, labelled You, its line break kept.
		const mine = rows[0] as HTMLElement;
		expect(within(mine).getByText(en.channelYou, { selector: "strong" }));
		const text = within(mine).getByText(/Two lines/);
		expect(text.textContent).toBe("Two lines:\nthe second.");
		expect(text.classList).toContain(own.lines);
		const hers = rows[1] as HTMLElement;
		expect(within(hers).getByRole("img").getAttribute("alt")).toBe("Mira");
		expect(within(hers).getByText("Yes.")).toBeTruthy();
	});

	it("opens_a_one_to_one_at_the_newest_message", async () => {
		stubBox();
		await chats("/channel/mira", {
			messages: Array.from({ length: 20 }, (_, i) =>
				chat("mira", `Line ${i}.`),
			),
		});
		const region = await screen.findByRole("region", {
			name: "Talking with Mira",
		});
		expect(region.scrollTop).toBe(2000);
	});

	it("follows_your_own_message_in_a_one_to_one", async () => {
		stubBox();
		const { s } = await chats("/channel/mira", {
			messages: Array.from({ length: 20 }, (_, i) =>
				chat("mira", `Line ${i}.`),
			),
		});
		const region = await screen.findByRole("region", {
			name: "Talking with Mira",
		});
		region.scrollTop = 300;
		fireEvent.scroll(region);
		live(s, 9000, "chat_message.posted", {
			chat: "mira",
			author: "human",
			text: "Could it?",
		});
		await screen.findByText("Could it?");
		expect(region.scrollTop).toBe(2000);
		expect(
			screen.queryByRole("button", { name: en.chatsNewMessages }),
		).toBeNull();
	});

	it("pages_back_in_a_one_to_one", async () => {
		const { s } = await chats("/channel/mira", { messages: fullPage() });
		const more = await screen.findByRole("button", { name: en.channelEarlier });
		fireEvent.click(more);
		const earlier = await waitFor(() => {
			const f = asked(s)[1];
			if (!f) throw new Error("no earlier page was asked");
			return f;
		});
		expect(earlier.params.params).toEqual({
			agent_id: "mira",
			before_seq: 201,
			limit: 100,
		});
		await s.reply(earlier, {
			messages: [198, 199, 200].map((n) =>
				chat("mira", `Older ${n}.`, { seq: n }),
			),
			waiting: null,
		});
		await screen.findByText("Older 198.");
		const rows = within(
			screen.getByRole("list", { name: en.channelMessages }),
		).getAllByRole("listitem");
		expect(rows.slice(0, 4).map((li) => li.textContent)).toEqual([
			expect.stringContaining("Older 198."),
			expect.stringContaining("Older 199."),
			expect.stringContaining("Older 200."),
			expect.stringContaining("Newer 0."),
		]);
		expect(
			screen.queryByRole("button", { name: en.channelEarlier }),
		).toBeNull();
	});

	it("pages_back_twice_in_a_one_to_one", async () => {
		const { s } = await chats("/channel/mira", { messages: fullPage() });
		fireEvent.click(
			await screen.findByRole("button", { name: en.channelEarlier }),
		);
		const first = await waitFor(() => {
			const f = asked(s)[1];
			if (!f) throw new Error("no earlier page was asked");
			return f;
		});
		await s.reply(first, {
			messages: Array.from({ length: 100 }, (_, i) =>
				chat("mira", `Older ${i}.`, { seq: 101 + i }),
			),
			waiting: null,
		});
		await screen.findByText("Older 0.");
		const button = await screen.findByRole("button", {
			name: en.channelEarlier,
		});
		expect((button as HTMLButtonElement).disabled).toBe(false);
		fireEvent.click(button);
		const second = await waitFor(() => {
			const f = asked(s)[2];
			if (!f) throw new Error("no third page was asked");
			return f;
		});
		expect(second.params.params).toEqual({
			agent_id: "mira",
			before_seq: 101,
			limit: 100,
		});
	});

	it("asks_earlier_at_the_top_of_a_one_to_one", async () => {
		stubBox();
		const { s } = await chats("/channel/mira", { messages: fullPage() });
		const region = await screen.findByRole("region", {
			name: "Talking with Mira",
		});
		region.scrollTop = 0;
		fireEvent.scroll(region);
		fireEvent.scroll(region);
		const earlier = await waitFor(() => {
			const f = asked(s)[1];
			if (!f) throw new Error("no earlier page was asked");
			return f;
		});
		expect(earlier.params.params).toEqual({
			agent_id: "mira",
			before_seq: 201,
			limit: 100,
		});
		expect(asked(s)).toHaveLength(2);
	});

	it("keeps_more_after_a_live_message", async () => {
		const { s } = await chats("/channel/mira", { messages: fullPage() });
		await screen.findByRole("button", { name: en.channelEarlier });
		live(s, 400, "chat_message.posted", {
			chat: "mira",
			author: "mira",
			text: "On it.",
		});
		const again = await waitFor(() => {
			const f = asked(s)[1];
			if (!f) throw new Error("the newest page was not asked again");
			return f;
		});
		await s.reply(again, {
			messages: [
				chat("human", "Hm.", { seq: 399 }),
				chat("mira", "On it.", { seq: 400 }),
			],
			waiting: null,
		});
		await screen.findByText("On it.");
		expect(
			screen.getByRole("button", { name: en.channelEarlier }),
		).toBeTruthy();
	});

	it("has_no_earlier_button_under_a_full_page", async () => {
		await chats("/channel/mira", {
			messages: Array.from({ length: 5 }, (_, i) => chat("mira", `Line ${i}.`)),
		});
		await screen.findByText("Line 0.");
		expect(
			screen.queryByRole("button", { name: en.channelEarlier }),
		).toBeNull();
	});

	it("says_when_an_earlier_chat_page_cannot_be_read", async () => {
		const { s } = await chats("/channel/mira", { messages: fullPage() });
		fireEvent.click(
			await screen.findByRole("button", { name: en.channelEarlier }),
		);
		const earlier = await waitFor(() => {
			const f = asked(s)[1];
			if (!f) throw new Error("no earlier page was asked");
			return f;
		});
		await s.fail(earlier, -32000, "the store is busy");
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.channelEarlierFailed,
		);
	});

	it("keeps_the_chat_box_to_one_row_on_a_phone", async () => {
		media.set(WIDE, false);
		await chats("/channel/mira");
		const hidden = () =>
			[
				screen.getByText("Message Mira"),
				document.getElementById("chat-enter") as HTMLElement,
			].map((x) => x.classList);
		await screen.findByRole("textbox", { name: "Message Mira" });
		for (const classes of hidden()) expect(classes).toContain(channel.hidden);
		cleanup();

		media.set(WIDE, true);
		await chats("/channel/mira");
		await screen.findByRole("textbox", { name: "Message Mira" });
		for (const classes of hidden())
			expect(classes).not.toContain(channel.hidden);
	});

	it("sends_a_chat", async () => {
		const { s } = await chats("/channel/mira");
		const box = (await screen.findByRole("textbox", {
			name: "Message Mira",
		})) as HTMLTextAreaElement;
		expect(screen.getByText(en.chatsEnterSends)).toBeTruthy();

		// Send sends.
		fireEvent.change(box, { target: { value: "Could it?" } });
		fireEvent.click(screen.getByRole("button", { name: en.chatsSend }));
		const sent = await waitFor(() => {
			const f = s.calls("command")[0];
			if (!f) throw new Error("no command was sent");
			return f;
		});
		expect(sent.params).toEqual({
			command: {
				command: "chat_message_post",
				body: { agent_id: "mira", text: "Could it?" },
			},
		});
		await s.reply(sent, { said: "posted", events: [9] });
		await waitFor(() => expect(box.value).toBe(""));

		// Shift+Enter is a new line, not a send; Enter sends.
		fireEvent.change(box, { target: { value: "One\ntwo" } });
		fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
		expect(s.calls("command")).toHaveLength(1);
		fireEvent.keyDown(box, { key: "Enter" });
		await waitFor(() => expect(s.calls("command")).toHaveLength(2));
		expect(s.calls("command")[1]?.params).toEqual({
			command: {
				command: "chat_message_post",
				body: { agent_id: "mira", text: "One\ntwo" },
			},
		});
		await s.reply(s.calls("command")[1] as never, {
			error: { kind: "refused", detail: "agent_retired: mira is retired" },
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseAgentRetired,
		);

		// 4,000 characters, counted as Catervas counts them, pass; 4,001 are refused before sending.
		fireEvent.change(box, { target: { value: "😀".repeat(4000) } });
		fireEvent.keyDown(box, { key: "Enter" });
		await waitFor(() => expect(s.calls("command")).toHaveLength(3));
		await s.reply(s.calls("command")[2] as never, {
			said: "posted",
			events: [10],
		});
		fireEvent.change(box, { target: { value: "😀".repeat(4001) } });
		fireEvent.click(screen.getByRole("button", { name: en.chatsSend }));
		expect((await screen.findByRole("alert")).textContent).toBe(
			"A message is at most 4000 characters, and this one is 4001.",
		);
		expect(s.calls("command")).toHaveLength(3);

		// The team's composer sends on Enter too.
		cleanup();
		const team = await chats("/channel");
		const post = await screen.findByRole("textbox", {
			name: en.channelPostPhone,
		});
		expect(screen.getByText(en.chatsEnterSends)).toBeTruthy();
		fireEvent.change(post, { target: { value: "Morning, all." } });
		fireEvent.keyDown(post, { key: "Enter", shiftKey: true });
		expect(team.s.calls("command")).toHaveLength(0);
		fireEvent.keyDown(post, { key: "Enter" });
		await waitFor(() =>
			expect(team.s.calls("command")[0]?.params).toEqual({
				command: { command: "message_post", body: { text: "Morning, all." } },
			}),
		);
	});

	it("sends_a_message_as_a_task", async () => {
		const { s } = await chats("/channel/mira");
		const box = (await screen.findByRole("textbox", {
			name: "Message Mira",
		})) as HTMLTextAreaElement;
		const text = "Add a dark mode switch to the settings page";
		fireEvent.change(box, { target: { value: text } });
		fireEvent.click(screen.getByRole("button", { name: en.chatsSendTask }));
		const sent = await waitFor(() => {
			const f = s.calls("command")[0];
			if (!f) throw new Error("no command was sent");
			return f;
		});
		expect(sent.params).toEqual({
			command: {
				command: "chat_message_post",
				body: { agent_id: "mira", text, as_task: true },
			},
		});
		await s.reply(sent, {
			said: "given to Mira as CTV-7",
			events: [9, 10, 11],
		});
		await waitFor(() => expect(box.value).toBe(""));

		// What it does is behind the info button.
		const info = screen.getByRole("button", { name: "More about this" });
		expect(info.getAttribute("aria-describedby")).toBe("chat-task-info");
		fireEvent.click(info);
		expect(info.getAttribute("aria-expanded")).toBe("true");
		expect(document.getElementById("chat-task-info")?.textContent).toBe(
			en.chatsTaskInfo.replace("{name}", "Mira"),
		);

		// A refusal reads in plain words.
		fireEvent.change(box, { target: { value: "too short" } });
		fireEvent.click(screen.getByRole("button", { name: en.chatsSendTask }));
		await waitFor(() => expect(s.calls("command")).toHaveLength(2));
		await s.reply(s.calls("command")[1] as never, {
			error: {
				kind: "refused",
				detail: "task_too_short: say a little more: at least 20 characters",
			},
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseTaskTooShort,
		);
	});

	it("shows_the_task_a_message_became", async () => {
		const { container } = await chats("/channel/mira", {
			messages: [
				chat("human", "Add a dark mode switch to the settings page", {
					sent_as: "CTV-7",
				}),
			],
			waiting: null,
		});
		const link = await screen.findByRole("link", {
			name: "Given to Mira as CTV-7",
		});
		expect(link.getAttribute("href")).toBe("/tasks/CTV-7");
		expect(
			screen.queryByText(en.chatsAnswering.replace("{name}", "Mira")),
		).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("shows_no_such_teammate", async () => {
		const { s } = await chats("/channel/nobody");
		expect(await screen.findByText(en.chatsNoSuch)).toBeTruthy();
		expect(screen.getByRole("navigation", { name: en.chats })).toBeTruthy();
		expect(screen.queryByRole("textbox")).toBeNull();
		expect(asked(s)).toHaveLength(0);
	});

	it("appends_chat_replies_live", async () => {
		const question = chat("human", "Could it?");
		const { s } = await chats("/channel/mira", { messages: [question] });
		await screen.findByText("Could it?");
		live(s, 90, "chat_message.posted", {
			chat: "mira",
			author: "mira",
			text: "On it.",
			in_reply_to: question.seq,
		});
		// Shown with no query answered for it.
		expect(await screen.findByText("On it.")).toBeTruthy();
		const before = asked(s).length;
		live(
			s,
			91,
			"chat_message.posted",
			{ chat: "theo", author: "theo", text: "Theo's own." },
			{ agent_id: "theo" },
		);
		expect(screen.queryByText("Theo's own.")).toBeNull();
		expect(asked(s)).toHaveLength(before);
		// A page that also holds it shows it once.
		await answerQuery(s, "chat.messages", {
			messages: [
				question,
				{ ...chat("mira", "On it."), seq: 90, in_reply_to: question.seq },
			],
			waiting: null,
		});
		expect(screen.getAllByText("On it.")).toHaveLength(1);
	});

	it("sends_a_proposal_as_a_request", async () => {
		const sent = chat("mira", "Here is one.", {
			request: { title: "Show sold-out items", text: "Grey them out." },
			sent_as: "CTV-12",
		});
		const reply = chat("mira", "Yes.", { request: APPLE });
		const other = chat("mira", "And this.", { request: APPLE });
		const { s } = await chats("/channel/mira", {
			messages: [sent, reply, other],
		});
		// One sent already: its title and a link, no box.
		expect(await screen.findByText(t("chatsSuggested"))).toBeTruthy();
		expect(screen.getByText("Show sold-out items")).toBeTruthy();
		expect(
			screen.getByRole("link", { name: "Sent as CTV-12" }).getAttribute("href"),
		).toBe("/requests/CTV-12");

		const boxes = screen.getAllByRole("textbox", {
			name: t("chatsSuggests"),
		}) as HTMLTextAreaElement[];
		expect(boxes).toHaveLength(2);
		const box = boxes[0] as HTMLTextAreaElement;
		expect(box.value).toBe(`${APPLE.title}\n\n${APPLE.text}`);
		fireEvent.change(box, { target: { value: "Pay with Apple Pay, please" } });
		const press = () =>
			fireEvent.click(
				within(box.closest("form") as HTMLElement).getByRole("button", {
					name: en.chatsSendRequest,
				}),
			);
		press();
		const filed = await waitFor(() => {
			const f = s.calls("request.file")[0];
			if (!f) throw new Error("no request.file was sent");
			return f;
		});
		expect(filed.params).toEqual({
			text: "Pay with Apple Pay, please",
			from_chat_message: reply.seq,
		});
		await s.fail(filed, -32005, "the request was already sent, as CTV-12");
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.requestSentAs.replace("{id}", "CTV-12"),
		);
		press();
		await waitFor(() => expect(s.calls("request.file")).toHaveLength(2));
		await s.reply(s.calls("request.file")[1] as never, { task_id: "CTV-13" });
		expect(
			(
				await screen.findByRole("link", { name: "Sent as CTV-13" })
			).getAttribute("href"),
		).toBe("/requests/CTV-13");

		// Sent from another tab: the event turns its box into the link.
		live(
			s,
			95,
			"task.created",
			{
				summary: { kind: "task", title: "Apple Pay", status: "draft" },
				created_by: "human",
				from_chat_message: other.seq,
			},
			{ task_id: "CTV-14", agent_id: undefined },
		);
		expect(
			await screen.findByRole("link", { name: "Sent as CTV-14" }),
		).toBeTruthy();
		expect(
			screen.queryAllByRole("textbox", { name: t("chatsSuggests") }),
		).toHaveLength(0);
	});

	it("says_why_no_answer", async () => {
		const { s } = await chats("/channel/mira", {
			messages: [chat("human", "Could it?")],
			waiting: { because: "answering" },
		});
		const status = () => screen.getByRole("status");
		await waitFor(() => expect(status().textContent).toBe("Mira is thinking…"));

		const until = "2026-09-30T15:40:00Z";
		const at = new Date(until)
			.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" })
			.toLowerCase();
		const reasons: [object, string, string?][] = [
			[
				{ because: "day_spent" },
				"Today’s spending limit is reached, so Mira will answer tomorrow. Your message is kept. See today’s costs",
				"/costs",
			],
			[
				{ because: "asleep", until },
				`Mira is resting until ${at}, when the AI account’s usage limit resets, and will answer then.`,
			],
			[
				{ because: "key_refused" },
				"Mira cannot answer: your AI provider refused the key. Check the key in Settings",
				"/settings/account",
			],
			[{ because: "no_answer" }, "Mira could not answer. Ask again."],
		];
		let next = 100;
		for (const [i, [waiting, words, href]] of reasons.entries()) {
			const n = asked(s).length;
			// A chat session of hers starting or ending asks again.
			if (i % 2 === 0)
				live(s, next++, "session.started", {
					purpose: "chat",
					chat: "mira",
					model: "m",
					effort: "low",
				});
			else
				live(s, next++, "session.ended", { reason: "completed", detail: "" });
			await waitFor(() => expect(asked(s)).toHaveLength(n + 1));
			await s.reply(asked(s).at(-1) as never, {
				messages: [],
				waiting,
			});
			await waitFor(() => expect(status().textContent).toBe(words));
			if (href)
				expect(within(status()).getByRole("link").getAttribute("href")).toBe(
					href,
				);
		}

		// Another agent's chat, or another purpose of hers, does not.
		const n = asked(s).length;
		live(s, next++, "session.started", {
			purpose: "implement",
			model: "m",
			effort: "low",
		});
		live(
			s,
			next++,
			"session.started",
			{ purpose: "chat", chat: "theo", model: "m", effort: "low" },
			{ agent_id: "theo" },
		);
		expect(asked(s)).toHaveLength(n);
		// Answered: no line.
		live(s, next++, "session.ended", { reason: "completed", detail: "" });
		await waitFor(() => expect(asked(s)).toHaveLength(n + 1));
		await s.reply(asked(s).at(-1) as never, { messages: [], waiting: null });
		await waitFor(() => expect(status().textContent).toBe(""));
	});

	it("keeps_a_past_teammate_read_only", async () => {
		await chats("/channel/kai", {
			messages: [chat("kai", "Bye.")],
			waiting: { because: "retired" },
		});
		expect(await screen.findByText("Bye.")).toBeTruthy();
		expect(screen.getByRole("status").textContent).toBe(
			"Kai has left the team. This chat is kept for you to read.",
		);
		expect(screen.queryByRole("textbox")).toBeNull();
		expect(screen.queryByRole("button", { name: en.chatsSend })).toBeNull();
	});

	it("renders an agent's reply as Markdown", async () => {
		media.set(WIDE, true);
		await chats("/channel/mira", {
			messages: [
				chat("mira", "Short answer.\n\n- **One** point\n- Two\n\nThat is all."),
			],
		});
		const messages = await screen.findByRole("list", {
			name: en.channelMessages,
		});
		const row = within(messages).getAllByRole("listitem")[0] as HTMLElement;
		expect(within(row).getAllByRole("listitem")).toHaveLength(2);
		expect(within(row).getByText("One").tagName).toBe("STRONG");
		expect(row.textContent).not.toContain("**");
		expect(row.textContent).not.toContain("- ");
	});

	it("keeps the user's own message as text", async () => {
		media.set(WIDE, true);
		await chats("/channel/mira", {
			messages: [chat("human", "- **not** a list")],
		});
		const text = await screen.findByText("- **not** a list");
		expect(text.textContent).toBe("- **not** a list");
		expect(within(text).queryByRole("list")).toBeNull();
		expect(text.classList).toContain(own.lines);
	});

	it("links a task id in an agent's reply", async () => {
		media.set(WIDE, true);
		const { s } = await chats("/channel/mira", {
			messages: [chat("mira", "CTV-7 is in review.")],
		});
		const link = await screen.findByRole("link", { name: "CTV-7" });
		expect(link.getAttribute("href")).toBe("/tasks/CTV-7");
		expect(fireEvent.click(link, { ctrlKey: true })).toBe(true);
		expect(fireEvent.click(link, { button: 1 })).toBe(true);
		expect(
			screen.getByRole("heading", { name: "Talking with Mira" }),
		).toBeTruthy();
		expect(fireEvent.click(link)).toBe(false);
		await waitFor(() =>
			expect(
				s
					.calls("query")
					.some(
						(f) =>
							f.params.name === "contract.get" &&
							JSON.stringify(f.params.params) ===
								JSON.stringify({ task_id: "CTV-7" }),
					),
			).toBe(true),
		);
		expect(
			screen.queryByRole("heading", { name: "Talking with Mira" }),
		).toBeNull();
	});

	it("leaves other links to the browser", async () => {
		media.set(WIDE, true);
		await chats("/channel/mira", {
			messages: [chat("mira", "[site](https://example.com/a)")],
		});
		const link = await screen.findByRole("link", { name: "site" });
		expect(fireEvent.click(link)).toBe(true);
		expect(
			screen.getByRole("heading", { name: "Talking with Mira" }),
		).toBeTruthy();
	});

	it("leaves task ids in code and links alone", async () => {
		media.set(WIDE, true);
		await chats("/channel/mira", {
			messages: [
				chat("mira", "`CTV-7` and [CTV-8](/tasks/CTV-8)\n\n```\nCTV-9\n```"),
			],
		});
		const code = await screen.findByText("CTV-7");
		expect(code.tagName).toBe("CODE");
		expect(code.closest("a")).toBeNull();
		const links = screen.getAllByRole("link", { name: "CTV-8" });
		expect(links).toHaveLength(1);
		expect(links[0]?.getAttribute("href")).toBe("/tasks/CTV-8");
		expect(document.querySelector("pre code")?.textContent).toBe("CTV-9\n");
		expect(screen.queryByRole("link", { name: "CTV-9" })).toBeNull();
	});

	it("renders_agent_text_as_text", async () => {
		media.set(WIDE, true);
		const evil = "<img src=x onerror=alert(1)>";
		const { container } = await chats("/channel/mira", {
			list: {
				team_last: null,
				chats: [
					{ agent_id: "mira", retired: false, last: line(1, "mira", evil) },
				],
			},
			messages: [
				chat("mira", evil),
				chat("mira", "See:", {
					request: { title: evil, text: "x".repeat(20) },
					sent_as: "CTV-1",
				}),
			],
		});
		const list = await screen.findByRole("navigation", { name: en.chats });
		expect(within(list).getByText(`Mira: ${evil}`)).toBeTruthy();
		const messages = screen.getByRole("list", { name: en.channelMessages });
		expect(within(messages).getAllByText(evil)).toHaveLength(2);
		expect(container.querySelector('img[src="x"]')).toBeNull();
	});

	const INTRO =
		"Before your team builds anything, Mira asks what you want to make. This chat only talks: your product plan is written next, and your team builds once it is done.";
	const SUMMARY = "Thank you, that’s enough to start. Here is what I heard.";
	const DRAFT = "Draft the product plan";
	const stateAsked = (s: FakeSocket) =>
		s.calls("query").filter((f) => f.params.name === "product_plan.state");

	it("introduces_the_interview", async () => {
		media.set(WIDE, true);
		await chats("/channel/mira", { plan: INTERVIEWING });
		expect(await screen.findByText(INTRO)).toBeTruthy();
		expect(screen.queryByText(en.chatsIntro.replaceAll("{name}", "Mira"))).toBe(
			null,
		);
		cleanup();

		// Approved, it is any chat's again.
		const approved = await chats("/channel/mira", { plan: APPROVED });
		expect(
			await screen.findByText(en.chatsIntro.replaceAll("{name}", "Mira")),
		).toBeTruthy();
		expect(screen.queryByText(INTRO)).toBeNull();
		expect(stateAsked(approved.s)).toHaveLength(1);
		cleanup();

		// A phone says it in its short place.
		media.set(WIDE, false);
		await chats("/channel/mira", { plan: INTERVIEWING });
		expect(await screen.findByText(INTRO)).toBeTruthy();
		expect(
			screen.queryByText(en.chatsIntroPhone.replace("{name}", "Mira")),
		).toBe(null);
		cleanup();

		// The Developer's chat never asks.
		media.set(WIDE, true);
		const developer = await chats("/channel/theo");
		await screen.findByRole("heading", { name: "Talking with Theo" });
		expect(stateAsked(developer.s)).toHaveLength(0);
	});

	it("shows_what_mira_heard", async () => {
		const { s } = await chats("/channel/mira", {
			messages: [
				chat("mira", SUMMARY, {
					heard: {
						...HEARD,
						problem: `${HEARD.problem} ‮ Stop.`,
						not_now: "One\ntwo",
					},
				}),
			],
			plan: APPROVED,
		});
		const region = await screen.findByRole("region", { name: "What I heard" });
		expect(
			within(region)
				.getAllByRole("term")
				.map((x) => x.textContent),
		).toEqual([
			"Your customers",
			"The problem",
			"It must",
			"Not now",
			"It works when",
		]);
		const values = within(region)
			.getAllByRole("definition")
			.map((x) => x.textContent);
		expect(values[0]).toBe(HEARD.customers);
		expect(values[2]).toBe(HEARD.must);
		expect(values[3]).toBe("One\ntwo");
		expect(values[4]).toBe(HEARD.works_when);
		showsWhatItHides(region);
		// Shown for any state: the button below follows the state alone.
		expect(within(region).queryByRole("button")).toBeNull();
		expect(s.calls("command")).toHaveLength(0);
	});

	it("drafts_the_plan_from_the_newest_message", async () => {
		const { s } = await chats("/channel/mira", {
			messages: [
				chat("human", "No lost orders."),
				chat("mira", SUMMARY, { heard: HEARD }),
			],
			plan: INTERVIEWING,
		});
		const region = await screen.findByRole("region", { name: "What I heard" });
		const button = within(region).getByRole("button", { name: DRAFT });
		expect(screen.getAllByRole("button", { name: DRAFT })).toHaveLength(1);
		expect(
			within(region).getByText(
				"I write it as my first task: a product description and a roadmap. Your team starts building once it is done.",
			),
		).toBeTruthy();
		fireEvent.click(button);
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: { command: "product_plan_draft", body: {} },
		});
		expect(refusedBy("emptyBody", bodyOf(sent))).toEqual([]);
		await s.reply(sent, { said: "drafting", events: [9] });

		// The filing's event makes the state asked again, and its task is named.
		await eventArrives(s, 99);
		await answerQuery(s, "product_plan.state", {
			...INTERVIEWING,
			task: {
				task_id: "CTV-1",
				status: "draft",
				created_at: "2026-09-28T09:50:00Z",
			},
		});
		const sentLine = await screen.findByText(
			"Drafting it as CTV-1. Your team starts building once it is done.",
		);
		expect(sentLine.closest("[role=status]")).toBe(sentLine);
		expect(screen.queryByRole("button", { name: DRAFT })).toBeNull();
	});

	it("puts_the_draft_button_on_her_newest_message_only", async () => {
		await chats("/channel/mira", {
			messages: [
				chat("mira", "Who are your customers?"),
				chat("human", "People nearby."),
				chat("mira", SUMMARY, { heard: HEARD }),
				chat("human", "One more thing"),
			],
			plan: INTERVIEWING,
		});
		const region = await screen.findByRole("region", { name: "What I heard" });
		const buttons = screen.getAllByRole("button", { name: DRAFT });
		expect(buttons).toHaveLength(1);
		expect(region.contains(buttons[0] as HTMLElement)).toBe(true);
		const last = screen
			.getByText("One more thing")
			.closest("li") as HTMLElement;
		expect(within(last).queryByRole("button")).toBeNull();
	});

	it("introduces_no_interview_for_an_existing_project", async () => {
		media.set(WIDE, true);
		await chats("/channel/mira", {
			messages: [chat("mira", "Who are your customers?")],
			plan: { new_project: null, approved: false, task: null, documents: [] },
		});
		expect(
			await screen.findByText(en.chatsIntro.replaceAll("{name}", "Mira")),
		).toBeTruthy();
		expect(screen.queryByText(INTRO)).toBeNull();
		expect(screen.queryByRole("button", { name: DRAFT })).toBeNull();
	});

	it("shows_what_mira_heard_as_it_arrives", async () => {
		const { s } = await chats("/channel/mira", { plan: INTERVIEWING });
		await screen.findByRole("heading", { name: "Talking with Mira" });
		live(s, 90, "chat_message.posted", {
			chat: "mira",
			author: "mira",
			text: SUMMARY,
			heard: HEARD,
		});
		const region = await screen.findByRole("region", { name: "What I heard" });
		expect(within(region).getByRole("button", { name: DRAFT })).toBeTruthy();
	});

	it("puts_the_draft_button_under_a_reply_with_no_summary", async () => {
		await chats("/channel/mira", {
			messages: [chat("mira", "Who are your customers?")],
			plan: INTERVIEWING,
		});
		const button = await screen.findByRole("button", { name: DRAFT });
		expect(screen.queryByRole("region", { name: "What I heard" })).toBeNull();
		const row = button.closest("li") as HTMLElement;
		expect(within(row).getByText("Who are your customers?")).toBeTruthy();
	});

	it("shows_the_task_in_place_of_the_button_and_nothing_once_approved", async () => {
		const task = {
			task_id: "CTV-4",
			status: "refining",
			created_at: "2026-09-28T09:50:00Z",
		};
		await chats("/channel/mira", {
			messages: [chat("mira", SUMMARY, { heard: HEARD })],
			plan: { ...INTERVIEWING, task },
		});
		expect(
			await screen.findByText(
				"Drafting it as CTV-4. Your team starts building once it is done.",
			),
		).toBeTruthy();
		expect(screen.queryByRole("button", { name: DRAFT })).toBeNull();
		cleanup();

		await chats("/channel/mira", {
			messages: [chat("mira", SUMMARY, { heard: HEARD })],
			plan: APPROVED,
		});
		await screen.findByRole("region", { name: "What I heard" });
		expect(screen.queryByRole("button", { name: DRAFT })).toBeNull();
		expect(screen.queryByText(/Drafting it as/)).toBeNull();
	});

	it("a_refused_draft_says_why", async () => {
		const { s } = await chats("/channel/mira", {
			messages: [chat("mira", SUMMARY, { heard: HEARD })],
			plan: INTERVIEWING,
		});
		fireEvent.click(await screen.findByRole("button", { name: DRAFT }));
		await s.reply(await sentCommand(s), {
			error: {
				kind: "refused",
				detail: "product_plan_no_reviewer: no Architect or Scrum Master",
			},
		});
		expect(
			await screen.findByText(
				"An Architect or a Scrum Master checks the plan before you read it. Add one on the Team page.",
			),
		).toBeTruthy();
	});

	it("says_each_refusal_of_the_draft_in_words", async () => {
		const plan = [
			[
				"not_a_new_project",
				"This project was not started here, so it has no product plan to draft.",
			],
			["product_plan_approved", "Your product plan is approved already."],
			["product_plan_open", "Mira is drafting your product plan already."],
			[
				"product_plan_no_writer",
				"Your team has no Product Manager at work to write it.",
			],
			[
				"product_plan_no_reviewer",
				"An Architect or a Scrum Master checks the plan before you read it. Add one on the Team page.",
			],
		] as const;
		for (const [code, words] of plan) {
			const { s } = await chats("/channel/mira", {
				messages: [chat("mira", SUMMARY, { heard: HEARD })],
				plan: INTERVIEWING,
			});
			fireEvent.click(await screen.findByRole("button", { name: DRAFT }));
			await s.reply(await sentCommand(s), {
				error: { kind: "refused", detail: `${code}: x` },
			});
			expect((await screen.findByRole("alert")).textContent).toBe(words);
			cleanup();
		}
	});

	it("passes_axe_on_the_chats", async () => {
		media.set(WIDE, true);
		const wide = await chats("/channel");
		await screen.findByRole("navigation", { name: en.chats });
		await expectNoAxeViolations(wide.container);
		cleanup();

		const open = await chats("/channel/mira", {
			messages: [
				chat("human", "Could customers also pay with Apple Pay?"),
				chat("human", "Add a dark mode switch to the settings page", {
					sent_as: "CTV-7",
				}),
				chat("mira", "Yes.", { request: APPLE }),
				chat("mira", "Done.", { request: APPLE, sent_as: "CTV-12" }),
				chat("mira", "Short answer.\n\n- **One**\n- Two\n\nCTV-12 is next."),
				chat("mira", "Here is what I heard.", { heard: HEARD }),
			],
			waiting: { because: "day_spent" },
			plan: INTERVIEWING,
		});
		await screen.findByRole("heading", { name: "Talking with Mira" });
		await screen.findByRole("button", { name: "Draft the product plan" });
		await expectNoAxeViolations(open.container);
		cleanup();

		// A phone: the row of faces above the chat.
		media.set(WIDE, false);
		const phone = await chats("/channel/mira", {
			messages: [
				chat("mira", "Yes.", { request: APPLE }),
				chat("mira", "Here is what I heard.", { heard: HEARD }),
			],
			waiting: { because: "answering" },
			plan: INTERVIEWING,
		});
		await screen.findByRole("heading", { name: "Talking with Mira" });
		await screen.findByRole("button", { name: "Draft the product plan" });
		await expectNoAxeViolations(phone.container);
	});
});

/** A string with `{name}` filled with Mira, as the page fills it. */
function t(key: "chatsSuggested" | "chatsSuggests"): string {
	return en[key].replace("{name}", "Mira");
}
