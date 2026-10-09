import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { sentCommand } from "../test/gate.ts";
import {
	KNOWN,
	MAILBOX,
	MARKUP,
	MESSAGE,
	NO_MAILBOX,
	ORDER_REPLY,
	ORDER_ROW,
	ORDERS_MESSAGE,
	REPLY,
	todayWithMail,
} from "../test/sellerMail.ts";

const SEND = {
	message: 5,
	to: "orders@pieboxpros.test",
	domain: "pieboxpros.test",
	new_domain: false,
	subject: "Order PO-12",
	body: "Please find our order attached.",
};

afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

describe("Today, messages to sellers", () => {
	it("shows_each_message_whole", async () => {
		const { container } = await todayWithMail({
			messages: [MESSAGE, KNOWN, ORDERS_MESSAGE],
		});
		const section = await screen.findByRole("region", {
			name: "Messages to sellers (2)",
		});
		expect(within(section).getByText(en.sellerLead)).toBeTruthy();
		const rows = within(section).getAllByRole("listitem");
		// An order's message is sent from its order, not here.
		expect(rows).toHaveLength(2);
		const first = within(rows[0] as HTMLElement);
		// Everything the agent wrote is text: the markup shows as typed, the hidden character is
		// written out, and the body sits in a frame that says whose words it is.
		expect(
			first.getByText(/Quote for 500 printed pie boxes <b>not bold<\/b>/),
		).toBeTruthy();
		const frame = first.getByText(/Could you quote 500 printed pie boxes/);
		expect(frame.tagName).toBe("FIELDSET");
		expect(frame.getAttribute("data-trust")).toBe("untrusted");
		expect(frame.textContent).toContain("<b>not bold</b>\\u{202e}");
		expect(frame.querySelector("b")).toBeNull();
		// The domain is in bold and the code face; a domain nothing went to before is said.
		expect(first.getByText("packagingexpress.test").tagName).toBe("STRONG");
		expect(
			first.getByText(
				/No message from Farik has gone to packagingexpress.test before/,
			),
		).toBeTruthy();
		const second = within(rows[1] as HTMLElement);
		expect(second.queryByText(/has gone to/)).toBeNull();
		expect(second.getByText("Ivo asks Pie Box Pros a question")).toBeTruthy();
		// What Farik adds under every message is shown with it.
		expect(
			first.getByText(/Written with an AI assistant and sent by Sam Ortiz/),
		).toBeTruthy();
		expect(first.getByText(/Corner Bakery/)).toBeTruthy();
		expect(first.getByText("buying@cornerbakery.test")).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("send_and_discard_act_at_once", async () => {
		const { s } = await todayWithMail({ messages: [KNOWN] });
		const row = within(
			(await screen.findAllByRole("listitem"))[0] as HTMLElement,
		);
		fireEvent.click(row.getByRole("button", { name: "Send" }));
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "seller_message_send",
				body: {
					message: 2,
					subject: "Delivery date",
					body: "When would 500 boxes arrive?",
				},
			},
		});
		await s.reply(sent, { said: "Sent to Pie Box Pros.", events: [90] });
		fireEvent.click(row.getByRole("button", { name: "Discard" }));
		const discarded = await sentCommand(s, 2);
		expect(discarded.params).toEqual({
			command: { command: "seller_message_discard", body: { message: 2 } },
		});
		// A refusal is said in words.
		await s.reply(discarded, {
			error: { kind: "refused", detail: "seller_message_sent: sent already" },
		});
		expect((await row.findByRole("alert")).textContent).toBe(
			en.refuseSellerMessageSent,
		);
	});

	it("no_mailbox_or_the_cap_means_no_send", async () => {
		await todayWithMail({ messages: [KNOWN], mailbox: NO_MAILBOX });
		const row = within(
			(await screen.findAllByRole("listitem"))[0] as HTMLElement,
		);
		expect(
			(row.getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled,
		).toBe(true);
		expect(screen.getByText(en.sellerNoMailbox, { exact: false })).toBeTruthy();
		expect(
			screen.getByRole("link", { name: en.mailboxConnectLink }),
		).toBeTruthy();
		expect(
			(row.getByRole("button", { name: "Discard" }) as HTMLButtonElement)
				.disabled,
		).toBe(false);
	});

	it("the_cap_leaves_no_send", async () => {
		await todayWithMail({
			messages: [KNOWN],
			mailbox: { ...MAILBOX, sent_today: 50 },
		});
		const row = within(
			(await screen.findAllByRole("listitem"))[0] as HTMLElement,
		);
		await screen.findByText(en.sellerCap);
		expect(
			(row.getByRole("button", { name: "Send" }) as HTMLButtonElement).disabled,
		).toBe(true);
	});

	it("a_failed_try_is_said_on_the_row", async () => {
		await todayWithMail({
			messages: [{ ...KNOWN, why: `The server refused it ${MARKUP}` }],
		});
		const alert = await screen.findByRole("alert");
		expect(alert.textContent).toBe(
			"Farik could not send it: The server refused it <b>not bold</b>. It is kept here to try again.",
		);
	});
});

describe("Today, replies from sellers", () => {
	it("a_reply_offers_comparison_or_follow_up", async () => {
		await todayWithMail({
			messages: [MESSAGE, ORDERS_MESSAGE],
			replies: [REPLY, ORDER_REPLY],
		});
		const section = await screen.findByRole("region", {
			name: "Replies from sellers (2)",
		});
		const [plain, order] = within(section).getAllByRole("listitem");
		const first = within(plain as HTMLElement);
		expect(
			first.getByText(/replied to “Quote for 500 printed pie boxes”/),
		).toBeTruthy();
		expect(first.getByText("1 attachment kept")).toBeTruthy();
		expect(first.getByText(/Anyone can write any From address/)).toBeTruthy();
		expect(
			first.getByText(/Dana Reyes <sales@packagingexpress.test>/).tagName,
		).toBe("CODE");
		expect(first.getByRole("button", { name: en.replyCompare })).toBeTruthy();
		expect(first.queryByRole("button", { name: en.ordersFollowUp })).toBeNull();
		const second = within(order as HTMLElement);
		expect(
			second.getByRole("button", { name: en.ordersFollowUp }),
		).toBeTruthy();
		expect(second.queryByRole("button", { name: en.replyCompare })).toBeNull();
	});

	it("dismiss_acts_at_once", async () => {
		const { s } = await todayWithMail({
			messages: [MESSAGE],
			replies: [REPLY],
		});
		fireEvent.click(await screen.findByRole("button", { name: "Dismiss" }));
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: { command: "seller_reply_dismiss", body: { reply: 1 } },
		});
	});
});

describe("an order's press", () => {
	const waitingOrder = (send: object | undefined) => [
		{ ...ORDER_ROW, ...(send ? { send } : {}) },
	];

	it("approve_and_send_shows_the_order_s_email", async () => {
		await todayWithMail({ waiting: waitingOrder(SEND) });
		const list = await screen.findByRole("list", { name: en.waitingList });
		expect(
			await within(list).findByRole("button", {
				name: "Approve and send to Pie Box Pros",
			}),
		).toBeTruthy();
		expect(
			within(list).getByRole("button", { name: en.orderApprove }),
		).toBeTruthy();
	});

	it("approve_and_send_needs_the_email_and_a_mailbox", async () => {
		await todayWithMail({ waiting: waitingOrder(undefined) });
		const list = await screen.findByRole("list", { name: en.waitingList });
		await within(list).findByRole("button", { name: en.orderApprove });
		expect(
			within(list).queryByRole("button", { name: /Approve and send/ }),
		).toBeNull();
		await waitFor(() =>
			expect(
				screen.queryByRole("button", { name: /Approve and send/ }),
			).toBeNull(),
		);
	});

	it("approve_and_send_needs_a_mailbox", async () => {
		await todayWithMail({ waiting: waitingOrder(SEND), mailbox: NO_MAILBOX });
		const list = await screen.findByRole("list", { name: en.waitingList });
		await within(list).findByRole("button", { name: en.orderApprove });
		expect(
			within(list).queryByRole("button", { name: /Approve and send/ }),
		).toBeNull();
	});
});
