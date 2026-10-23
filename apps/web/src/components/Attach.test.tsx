import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { todayWith } from "../test/posts.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const SPEC = "docs/catervas/architecture/attached/spec.md";
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
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};

const pdf = (name = "spec.pdf") =>
	new File(["pdf-bytes"], name, { type: "application/pdf" });

/** Picks `file` with the paperclip's hidden input, answers "What is it about?" and returns its call. */
async function pick(
	container: HTMLElement,
	s: FakeSocket,
	file: File,
	about: "product" | "architecture" = "product",
) {
	const before = s.calls("document.attach").length;
	const input = container.querySelector('input[type="file"]');
	if (!input) throw new Error("no file input");
	fireEvent.change(input, { target: { files: [file] } });
	const dialog = await screen.findByRole("dialog", {
		name: en.attachKindQuestion,
	});
	const product = within(dialog).getByRole("radio", {
		name: en.attachKindProduct,
	}) as HTMLInputElement;
	expect(product.checked).toBe(true);
	if (about === "architecture")
		fireEvent.click(
			within(dialog).getByRole("radio", { name: en.attachKindArchitecture }),
		);
	fireEvent.click(within(dialog).getByRole("button", { name: en.attachSend }));
	await waitFor(() =>
		expect(s.calls("document.attach")).toHaveLength(before + 1),
	);
	return s.calls("document.attach")[before] as never;
}

describe("attaching a document", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("attaches_a_document_and_sends_it_with_the_message", async () => {
		// Today's request box.
		vi.useFakeTimers({ toFake: ["Date"] });
		const { container, s } = await todayWith({});
		vi.useRealTimers();
		expect(screen.getByRole("button", { name: en.attachButton })).toBeTruthy();
		const input = container.querySelector(
			'input[type="file"]',
		) as HTMLInputElement;
		expect(input.accept).toBe(".md,.markdown,.txt,.pdf,.docx");

		const first = await pick(container, s, pdf(), "architecture");
		expect((first as { params: unknown }).params).toEqual({
			name: "spec.pdf",
			kind: "architecture",
			data_base64: btoa("pdf-bytes"),
		});
		// While it is being added the chip shows a spinner and the box does not send.
		const chips = screen.getByRole("list", { name: en.attachedList });
		expect(within(chips).getByText("spec.pdf")).toBeTruthy();
		expect(
			(
				screen.getByRole("button", {
					name: en.requestSend,
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);
		await s.reply(first, { path: SPEC, change: 3 });
		const remove = within(chips).getByRole("button", {
			name: en.attachRemove.replace("{name}", "spec.pdf"),
		});
		expect(remove).toBeTruthy();

		fireEvent.change(screen.getByRole("textbox"), {
			target: { value: "Build what the spec says." },
		});
		fireEvent.click(screen.getByRole("button", { name: en.requestSend }));
		const filed = await waitFor(() => {
			const f = s.calls("request.file")[0];
			if (!f) throw new Error("no request was filed");
			return f;
		});
		expect(filed.params).toEqual({
			text: "Build what the spec says.",
			attachments: [SPEC],
		});

		// A refusal is said in the code's words, and leaves no chip.
		const scan = await pick(container, s, pdf("scan.pdf"));
		await s.fail(scan, -32005, "refused", {
			errors: [{ path: "/data_base64", message: "no text", code: "no_text" }],
		});
		expect((await screen.findByRole("alert")).textContent).toContain(
			en.attach_no_text,
		);
		expect(screen.queryByText("scan.pdf")).toBeNull();

		// A sixth file is refused without a call: four more make five.
		for (const name of ["a.pdf", "b.pdf", "c.pdf", "d.pdf"]) {
			const call = await pick(container, s, pdf(name));
			await s.reply(call, {
				path: `docs/catervas/product/attached/${name}`,
				change: 1,
			});
		}
		expect(within(chips).getAllByRole("listitem")).toHaveLength(5);
		const calls = s.calls("document.attach").length;
		const input6 = container.querySelector('input[type="file"]');
		fireEvent.change(input6 as Element, { target: { files: [pdf("e.pdf")] } });
		fireEvent.click(
			within(await screen.findByRole("dialog")).getByRole("button", {
				name: en.attachSend,
			}),
		);
		expect((await screen.findByRole("alert")).textContent).toContain(
			en.attach_too_many,
		);
		expect(s.calls("document.attach")).toHaveLength(calls);
	});

	it("sends_a_chats_and_the_channels_attachments_and_shows_them_on_sent_messages", async () => {
		// The one-to-one chat: a sent message shows its chip, which opens the document.
		const { container, socket } = await renderApp("/channel/mira");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", { team: TEAM });
		await answerQuery(s, "chats.list", { team_last: null, chats: [] });
		await answerQuery(s, "chat.messages", {
			messages: [
				{
					seq: 1,
					at: "2026-09-28T09:40:00Z",
					author: "human",
					text: "See the spec.",
					in_reply_to: null,
					request: null,
					heard: null,
					sent_as: null,
					attachments: [{ path: SPEC, name: "spec.pdf" }],
				},
			],
			waiting: null,
		});
		const link = await screen.findByRole("link", { name: "spec.pdf" });
		expect(link.getAttribute("href")).toBe(
			"/t/shop/files/architecture/attached/spec.md",
		);

		const call = await pick(container, s, pdf("plan.pdf"));
		await s.reply(call, { path: SPEC, change: 3 });
		fireEvent.change(
			await screen.findByRole("textbox", { name: "Message Mira" }),
			{
				target: { value: "Read it." },
			},
		);
		fireEvent.click(screen.getByRole("button", { name: en.chatsSend }));
		const sent = await waitFor(() => {
			const f = s.calls("command")[0];
			if (!f) throw new Error("no command was sent");
			return f;
		});
		expect(sent.params).toEqual({
			command: {
				command: "chat_message_post",
				body: { agent_id: "mira", text: "Read it.", attachments: [SPEC] },
			},
		});
		await s.reply(sent, { said: "posted", events: [9] });
		// The chips go once the message is sent.
		await waitFor(() => expect(screen.queryByText("plan.pdf")).toBeNull());
	});

	it("sends_the_channels_attachments", async () => {
		const { container, socket } = await renderApp("/channel");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", { team: TEAM });
		await answerQuery(s, "chats.list", { team_last: null, chats: [] });
		await answerQuery(s, "channel.messages", {
			messages: [
				{
					seq: 1,
					at: "2026-09-28T09:02:00Z",
					author: "human",
					kind: "human",
					text: "Spec attached.",
					mentions: [],
					thread: null,
					in_reply_to: null,
					task_id: null,
					attachments: [{ path: SPEC, name: "spec.pdf" }],
				},
			],
		});
		await answerQuery(s, "waiting.list", { waiting: [] });
		const link = await screen.findByRole("link", { name: "spec.pdf" });
		expect(link.getAttribute("href")).toBe(
			"/t/shop/files/architecture/attached/spec.md",
		);

		const call = await pick(container, s, pdf("plan.pdf"), "architecture");
		await s.reply(call, { path: SPEC, change: 3 });
		fireEvent.change(await screen.findByRole("textbox"), {
			target: { value: "Look at this." },
		});
		fireEvent.click(screen.getByRole("button", { name: en.channelPost }));
		const sent = await waitFor(() => {
			const f = s.calls("command")[0];
			if (!f) throw new Error("no command was sent");
			return f;
		});
		expect(sent.params).toEqual({
			command: {
				command: "message_post",
				body: { text: "Look at this.", attachments: [SPEC] },
			},
		});
	});
});
