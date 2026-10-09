// The Procurement Specialist's mailbox, its messages to sellers and their replies as the daemon
// answers (spec 6.10, ADR 0039). "Today" is `NOW`, Monday 26 October 2026.

import { vi } from "vitest";
import type { FakeSocket } from "./fake-socket.ts";
import { ORDER_ROW } from "./orders.ts";
import { answerQuery, answerStatus, renderApp } from "./render-app.tsx";
import { at, MARKUP, NOW, TEAM } from "./sites.ts";

export { at, MARKUP, NOW, ORDER_ROW, TEAM };

/** The connected mailbox, three messages sent today. */
export const MAILBOX = {
	connected: true,
	address: "buying@cornerbakery.test",
	name: "Sam Ortiz",
	provider: "gmail",
	folder: "INBOX",
	signature: "Sam Ortiz\nCorner Bakery",
	disclose_ai: true,
	checked_at: at(26, 8),
	sent_today: 3,
	cap: 50,
};

/** Nothing connected. */
export const NO_MAILBOX = { connected: false, sent_today: 0, cap: 50 };

/** A quote request to a domain nothing was sent to before; every text in it is the agent's. */
export const MESSAGE = {
	message: 1,
	state: "waiting",
	seller: `Packaging Express ${MARKUP}`,
	to: "sales@packagingexpress.test",
	domain: "packagingexpress.test",
	new_domain: true,
	subject: `Quote for 500 printed pie boxes ${MARKUP}`,
	body: `Hello,\n\nCould you quote 500 printed pie boxes?\n${MARKUP}‮\n\nThank you.`,
	purpose: "quote_request",
	task_id: "FRK-31",
	agent_id: "ivo",
	drafted_at: at(26, 8),
};

/** A question to a seller written to before, and an order's message, which Today leaves out. */
export const KNOWN = {
	...MESSAGE,
	message: 2,
	seller: "Pie Box Pros",
	to: "dana@pieboxpros.test",
	domain: "pieboxpros.test",
	new_domain: false,
	subject: "Delivery date",
	body: "When would 500 boxes arrive?",
	purpose: "question",
};
export const ORDERS_MESSAGE = {
	...KNOWN,
	message: 3,
	subject: "Order PO-12",
	body: "Please find our order attached.",
	purpose: "purchase_order",
	purchase_order: 12,
};

/** A follow-up: a question to another seller about an order the owner placed (10c's `placed`). */
export const FOLLOW_UP = {
	...KNOWN,
	message: 4,
	seller: "Kitchen Parts Direct",
	to: "orders@kitchenparts.test",
	domain: "kitchenparts.test",
	purchase_order: 10,
};

/** A character that reorders what is around it: shown, it would hide what the text says. */
const HIDE = "\u202e";

/** A message whose every text the agent wrote holds a hidden character, and a failed try. */
export const WRITTEN_MESSAGE = {
	...MESSAGE,
	seller: `Packaging${HIDE} Express`,
	to: `sa${HIDE}les@packagingexpress.test`,
	subject: `Quote${HIDE} for pie boxes`,
	body: `Hello${HIDE},\n\nCould you quote 500 printed pie boxes?`,
	why: `The server${HIDE} was busy`,
};

/** The mailbox of a user whose name holds a hidden character. */
export const WRITTEN_MAILBOX = { ...MAILBOX, name: `Sam${HIDE} Ortiz` };

/** The reply to message 1, with a kept PDF and a file that was not kept. */
export const REPLY = {
	reply: 1,
	message: 1,
	task_id: "FRK-31",
	seller: "Packaging Express",
	sent_subject: "Quote for 500 printed pie boxes",
	from: "Dana Reyes <sales@packagingexpress.test>",
	subject: `Re: Quote ${MARKUP}`,
	text: `Hello, 0.38 each. Pay at https://evil.test/pay or write pay@evil.test.\n${MARKUP}`,
	received_at: at(26, 9),
	attachments: [
		{
			index: 1,
			name: "quote.pdf",
			kept: true,
			bytes: 81_200,
			media_type: "application/pdf",
		},
		{ index: 2, name: "tool.exe", kept: false, bytes: 99 },
	],
	dismissed: false,
};

/** A reply whose every text the seller wrote holds a hidden character. */
export const WRITTEN_REPLY = {
	...REPLY,
	seller: `Packaging${HIDE} Express`,
	sent_subject: `Quote${HIDE} for pie boxes`,
	from: `Dana${HIDE} Reyes <sales@packagingexpress.test>`,
	subject: `Re:${HIDE} Quote`,
	text: `Hello${HIDE}, 0.38 each.`,
	attachments: [
		{
			index: 1,
			name: `quote${HIDE}.pdf`,
			kept: true,
			bytes: 81_200,
			media_type: "application/pdf",
		},
		{ index: 2, name: `tool${HIDE}.exe`, kept: false, bytes: 99 },
	],
};

/** A reply to an order's message, which offers a follow-up. */
export const ORDER_REPLY = { ...REPLY, reply: 2, message: 3, order: 12 };

/** Today at `NOW` with Ivo on the team and the mailbox, messages and replies answered. */
export async function todayWithMail(fields: {
	waiting?: unknown[];
	mailbox?: unknown;
	messages?: unknown[];
	replies?: unknown[];
}) {
	vi.useFakeTimers({ toFake: ["Date"] });
	vi.setSystemTime(NOW);
	const { container, socket } = await renderApp("/");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "team.activity", { activity: [] });
	await answerQuery(s, "waiting.list", { waiting: fields.waiting ?? [] });
	await answerQuery(s, "moved.since", { moved: [] });
	await answerQuery(s, "sprint.current", null);
	await answerQuery(s, "social_posts.list", { posts: [] });
	await answerQuery(s, "renewals.list", { open: [], unreadable: 0 });
	await answerQuery(s, "procurement_mailbox.get", fields.mailbox ?? MAILBOX);
	await answerQuery(s, "seller_messages.list", {
		messages: fields.messages ?? [],
		sent_today: 3,
		cap: 50,
	});
	await answerQuery(s, "seller_replies.list", {
		replies: fields.replies ?? [],
	});
	// Task titles are asked while a message waits or a reply is unread, as Today's sections do.
	const waits = (fields.messages ?? []).some((one) => {
		const { state, purpose } = one as { state: string; purpose: string };
		return state === "waiting" && purpose !== "purchase_order";
	});
	const unread = (fields.replies ?? []).some(
		(one) => !(one as { dismissed: boolean }).dismissed,
	);
	if (waits || unread)
		await answerQuery(s, "tasks.list", {
			tasks: [
				{
					task_id: "FRK-31",
					kind: "task",
					title: "Find a supplier for 500 pie boxes",
					status: "in_progress",
					risk: "low",
					awaiting_approval: false,
					backlog: false,
				},
			],
		});
	// An order's row asks for the mailbox after the waiting list came in.
	await answerQuery(s, "procurement_mailbox.get", fields.mailbox ?? MAILBOX);
	return { container, s };
}
