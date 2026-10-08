import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import { sentCommand } from "../../test/gate.ts";
import { ORDER_ROW, todayWithOrders } from "../../test/orders.ts";

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
				"You place the order and pay for it yourself: Farik never does. Then mark it placed on Ivo’s page, and Ivo follows it up until it comes.",
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
