import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import { sentCommand } from "../../test/gate.ts";
import { ORDER_ROW, todayWithOrders } from "../../test/orders.ts";
import { todayWithMail } from "../../test/sellerMail.ts";

/** Today with the order waiting; its row's own button opened. */
async function opened(
	button: "Approve, I’ll place it myself" | "Reject",
	dialogName: string,
	row: object = ORDER_ROW,
) {
	const { container, s } = await todayWithOrders({ waiting: [row] });
	const list = await screen.findByRole("list", { name: en.waitingList });
	fireEvent.click(
		within(within(list).getByRole("listitem")).getByRole("button", {
			name: button,
		}),
	);
	const dialog = await screen.findByRole("dialog", { name: dialogName });
	return { container, s, dialog };
}

describe("a purchase order's dialog", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("approve_sends_approve_with_the_note", async () => {
		const { container, s, dialog } = await opened(
			"Approve, I’ll place it myself",
			"Approve PO-12 from Pie Box Pros?",
		);
		expect(
			within(dialog).getByText(
				"You place this order and pay for it yourself; Farik never pays. Then mark it placed on Ivo’s page, and Ivo follows it up until it comes.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		// Closing decides nothing.
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);

		// With no note, none is sent.
		fireEvent.click(
			screen.getByRole("button", { name: "Approve, I’ll place it myself" }),
		);
		const again = await screen.findByRole("dialog", {
			name: "Approve PO-12 from Pie Box Pros?",
		});
		fireEvent.click(within(again).getByRole("button", { name: "Approve" }));
		const plain = await sentCommand(s);
		expect(plain.params).toEqual({
			command: {
				command: "purchase_order_decide",
				body: { order: 12, decision: "approve" },
			},
		});
		// A refusal is said in words, whatever the daemon's text is, and the dialog stays.
		await s.reply(plain, {
			error: {
				kind: "refused",
				detail: "purchase_order_decided: order 12 was decided already",
			},
		});
		expect((await within(again).findByRole("alert")).textContent).toBe(
			en.refusePurchaseOrderDecided,
		);

		// A note is trimmed.
		fireEvent.change(within(again).getByLabelText(/A note for Ivo/), {
			target: { value: "  Good choice. I’ll order it tonight.  " },
		});
		fireEvent.click(within(again).getByRole("button", { name: "Approve" }));
		const withNote = await sentCommand(s, 2);
		expect(withNote.params).toEqual({
			command: {
				command: "purchase_order_decide",
				body: {
					order: 12,
					decision: "approve",
					note: "Good choice. I’ll order it tonight.",
				},
			},
		});
		await s.reply(withNote, { said: "approved PO-12", events: [90] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("reject_sends_reject_with_the_note", async () => {
		const { container, s, dialog } = await opened(
			"Reject",
			"Reject PO-12 from Pie Box Pros?",
		);
		expect(
			within(dialog).getByText(
				"Ivo reads your note in its next piece of work and can set up another order.",
			),
		).toBeTruthy();
		// Rejecting is one button, named as the choice.
		expect(
			within(dialog)
				.getAllByRole("button")
				.map((button) => button.textContent),
		).toEqual(["Close", "Reject"]);
		await expectNoAxeViolations(container);

		fireEvent.click(within(dialog).getByRole("button", { name: "Reject" }));
		const plain = await sentCommand(s);
		expect(plain.params).toEqual({
			command: {
				command: "purchase_order_decide",
				body: { order: 12, decision: "reject" },
			},
		});
		await s.reply(plain, {
			error: {
				kind: "refused",
				detail: "purchase_order_expired: order 12 closed by itself",
			},
		});
		expect((await within(dialog).findByRole("alert")).textContent).toBe(
			en.refusePurchaseOrderExpired,
		);

		fireEvent.change(within(dialog).getByLabelText(/A note for Ivo/), {
			target: { value: "Too dear. Try the other two." },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Reject" }));
		const withNote = await sentCommand(s, 2);
		expect(withNote.params).toEqual({
			command: {
				command: "purchase_order_decide",
				body: {
					order: 12,
					decision: "reject",
					note: "Too dear. Try the other two.",
				},
			},
		});
		await s.reply(withNote, { said: "rejected PO-12", events: [91] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("names_the_seller_as_text_even_when_it_hides_something", async () => {
		const { dialog } = await opened(
			"Reject",
			"Reject PO-12 from Pie\\u{202e} Box <b>not bold</b>?",
			{ ...ORDER_ROW, seller: "Pie\u202e Box <b>not bold</b>" },
		);
		// What reorders text is written out, and markup stays text.
		expect(dialog.querySelector("b")).toBeNull();
	});
});

describe("a purchase order that can be emailed", () => {
	const SEND = {
		message: 5,
		to: "orders@pieboxpros.test",
		domain: "pieboxpros.test",
		new_domain: true,
		subject: "Order PO-12",
		body: "Please find our order attached.",
	};

	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	async function sending(send: object = SEND) {
		const { s } = await todayWithMail({ waiting: [{ ...ORDER_ROW, send }] });
		fireEvent.click(
			await screen.findByRole("button", {
				name: "Approve and send to Pie Box Pros",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Approve PO-12 and send it to Pie Box Pros?",
		});
		return { s, dialog };
	}

	it("approve_and_send_sends_then_asks_for_the_follow_up", async () => {
		const { s, dialog } = await sending();
		expect(
			within(dialog).getByText(
				/Farik emails this order to Pie Box Pros from your procurement mailbox/,
			),
		).toBeTruthy();
		expect(within(dialog).getByText("Attached: PO-12.xlsx")).toBeTruthy();
		expect(
			within(dialog).getByText(
				/No message from Farik has gone to pieboxpros.test before/,
			),
		).toBeTruthy();
		// A failed send files nothing, and the dialog says why.
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Approve and send" }),
		);
		const failed = await sentCommand(s);
		expect(failed.params).toEqual({
			command: {
				command: "purchase_order_send",
				body: {
					order: 12,
					message: 5,
					subject: "Order PO-12",
					body: "Please find our order attached.",
				},
			},
		});
		await s.reply(failed, {
			error: { kind: "failed", detail: "seller_message_failed: busy" },
		});
		expect((await within(dialog).findByRole("alert")).textContent).toBe(
			en.refuseSellerMessageFailed,
		);
		expect(s.calls("request.file")).toHaveLength(0);
		// Pressed again, it sends, with a note, then files the follow-up the owner left ticked.
		fireEvent.change(within(dialog).getByLabelText(/A note for Ivo/), {
			target: { value: " Thanks " },
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Approve and send" }),
		);
		const sent = await sentCommand(s, 2);
		expect(
			(sent.params as { command: { body: { note?: string } } }).command.body
				.note,
		).toBe("Thanks");
		await s.reply(sent, { said: "Sent PO-12.", events: [91, 92, 93] });
		await waitFor(() => expect(s.calls("request.file")).toHaveLength(1));
		expect(s.calls("request.file")[0]?.params.text).toBe(
			"Follow up on PO-12 from Pie Box Pros until it arrives. I placed it on 26 October.",
		);
	});

	it("unticked_asks_for_nothing", async () => {
		const { s, dialog } = await sending();
		fireEvent.click(within(dialog).getByLabelText(/Ask Ivo to follow up/));
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Approve and send" }),
		);
		const sent = await sentCommand(s);
		await s.reply(sent, { said: "Sent PO-12.", events: [91] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("request.file")).toHaveLength(0);
	});

	it("approve_alone_says_farik_never_pays", async () => {
		const { s } = await todayWithMail({
			waiting: [{ ...ORDER_ROW, send: SEND }],
		});
		fireEvent.click(
			await screen.findByRole("button", {
				name: "Approve, I’ll place it myself",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Approve PO-12 from Pie Box Pros?",
		});
		expect(
			within(dialog).getByText(
				/You place this order and pay for it yourself; Farik never pays/,
			),
		).toBeTruthy();
		expect(s.calls("command")).toHaveLength(0);
	});
});
