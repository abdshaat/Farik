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
import { media } from "../test/media.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
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
	fields: { messages?: unknown[]; waiting?: unknown; list?: unknown } = {},
) {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "chats.list", fields.list ?? LIST);
	if (path === "/channel") {
		await answerQuery(s, "channel.messages", { messages: [] });
		await answerQuery(s, "waiting.list", { waiting: [] });
		await answerQuery(s, "sprint.current", null);
	} else if (path !== "/channel/nobody")
		await answerQuery(s, "chat.messages", {
			messages: fields.messages ?? [],
			waiting: fields.waiting ?? null,
		});
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

describe("chats", () => {
	afterEach(() => vi.unstubAllGlobals());

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
			"Mira is retired",
		);

		// 4,000 characters, counted as Farik counts them, pass; 4,001 are refused before sending.
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
			sent_as: "FRK-12",
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
			screen.getByRole("link", { name: "Sent as FRK-12" }).getAttribute("href"),
		).toBe("/requests/FRK-12");

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
		await s.fail(filed, -32005, "the request was already sent, as FRK-12");
		expect((await screen.findByRole("alert")).textContent).toBe(
			"The request was already sent, as FRK-12",
		);
		press();
		await waitFor(() => expect(s.calls("request.file")).toHaveLength(2));
		await s.reply(s.calls("request.file")[1] as never, { task_id: "FRK-13" });
		expect(
			(
				await screen.findByRole("link", { name: "Sent as FRK-13" })
			).getAttribute("href"),
		).toBe("/requests/FRK-13");

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
			{ task_id: "FRK-14", agent_id: undefined },
		);
		expect(
			await screen.findByRole("link", { name: "Sent as FRK-14" }),
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
				"/settings",
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
					sent_as: "FRK-1",
				}),
			],
		});
		const list = await screen.findByRole("navigation", { name: en.chats });
		expect(within(list).getByText(`Mira: ${evil}`)).toBeTruthy();
		const messages = screen.getByRole("list", { name: en.channelMessages });
		expect(within(messages).getAllByText(evil)).toHaveLength(2);
		expect(container.querySelector('img[src="x"]')).toBeNull();
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
				chat("mira", "Yes.", { request: APPLE }),
				chat("mira", "Done.", { request: APPLE, sent_as: "FRK-12" }),
			],
			waiting: { because: "day_spent" },
		});
		await screen.findByRole("heading", { name: "Talking with Mira" });
		await expectNoAxeViolations(open.container);
		cleanup();

		// A phone: the row of faces above the chat.
		media.set(WIDE, false);
		const phone = await chats("/channel/mira", {
			messages: [chat("mira", "Yes.", { request: APPLE })],
			waiting: { because: "answering" },
		});
		await screen.findByRole("heading", { name: "Talking with Mira" });
		await expectNoAxeViolations(phone.container);
	});
});

/** A string with `{name}` filled with Mira, as the page fills it. */
function t(key: "chatsSuggested" | "chatsSuggests"): string {
	return en[key].replace("{name}", "Mira");
}
