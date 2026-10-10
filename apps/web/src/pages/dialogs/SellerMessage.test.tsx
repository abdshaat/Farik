import { expectNoAxeViolations } from "@catervas/ui/test";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import { sentCommand } from "../../test/gate.ts";
import { showsWhatItHides } from "../../test/hidden.ts";
import { bodyOf, refusedBy } from "../../test/schema.ts";
import {
	FAILED_AT,
	KNOWN,
	MAILBOX,
	NO_MAILBOX,
	todayWithMail,
	WRITTEN_MAILBOX,
	WRITTEN_MESSAGE,
} from "../../test/sellerMail.ts";

afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

async function edited(message: object = KNOWN, mailbox?: object) {
	const { container, s } = await todayWithMail({
		messages: [message],
		...(mailbox ? { mailbox } : {}),
	});
	fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
	const dialog = await screen.findByRole("dialog", {
		name: "Edit the message to Pie Box Pros",
	});
	return { container, s, dialog };
}

describe("a message to a seller, edited", () => {
	it("edit_then_send_sends_the_edited_text", async () => {
		const { container, s, dialog } = await edited();
		// On a phone the dialog fills the screen, and nothing in it breaks an accessibility rule.
		expect(dialog.hasAttribute("data-fills-phone")).toBe(true);
		await expectNoAxeViolations(container);
		// From and To stay as the agent wrote them, with what Catervas adds.
		expect(within(dialog).getByText("buying@cornerbakery.test")).toBeTruthy();
		expect(within(dialog).getByText("pieboxpros.test").tagName).toBe("STRONG");
		expect(
			within(dialog).getByText(/Written with an AI assistant/),
		).toBeTruthy();
		expect(
			within(dialog).getByText(
				"What you send is yours: Ivo reads the text you sent, not its draft.",
			),
		).toBeTruthy();
		fireEvent.change(within(dialog).getByLabelText("Subject"), {
			target: { value: "  Delivery date for 1,000  " },
		});
		fireEvent.change(within(dialog).getByLabelText("Message"), {
			target: { value: "When would 1,000 boxes arrive?" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Send" }));
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "seller_message_send",
				body: {
					message: 2,
					subject: "Delivery date for 1,000",
					body: "When would 1,000 boxes arrive?",
				},
			},
		});
		expect(refusedBy("sellerMessageSendBody", bodyOf(sent))).toEqual([]);
		await s.reply(sent, { said: "Sent to Pie Box Pros.", events: [90] });
		await vi.waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("the_edit_dialog_shows_what_the_agent_hid", async () => {
		await todayWithMail({
			messages: [WRITTEN_MESSAGE],
			mailbox: WRITTEN_MAILBOX,
		});
		fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
		// The title names the seller with the hidden character written out.
		const dialog = await screen.findByRole("dialog", {
			name: "Edit the message to Packaging\\u{202e} Express",
		});
		// The note on a new domain, the failed try, From and To: none hides what is around it.
		showsWhatItHides(dialog);
		expect(within(dialog).getByRole("alert").textContent).toContain(
			"The server\\u{202e} was busy",
		);
	});

	it("the_cap_or_no_mailbox_leaves_the_dialog_no_send", async () => {
		// At the day's cap the dialog says so, and offers no Send.
		const capped = await edited(KNOWN, { ...MAILBOX, sent_today: 50 });
		expect(within(capped.dialog).getByText(en.sellerCap)).toBeTruthy();
		expect(
			within(capped.dialog).queryByRole("button", { name: "Send" }),
		).toBeNull();
		fireEvent.click(
			within(capped.dialog).getByRole("button", { name: "Close" }),
		);
		await vi.waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		cleanup();
		// With no mailbox it says to connect one, and offers no Send.
		const none = await edited(KNOWN, NO_MAILBOX);
		expect(within(none.dialog).getByText(en.sellerNoMailbox)).toBeTruthy();
		expect(
			within(none.dialog).queryByRole("button", { name: "Send" }),
		).toBeNull();
	});

	it("nothing_to_send_or_too_much_leaves_the_dialog_no_send", async () => {
		const { dialog } = await edited();
		const send = () =>
			within(dialog).getByRole("button", { name: "Send" }) as HTMLButtonElement;
		const subject = within(dialog).getByLabelText("Subject");
		const message = within(dialog).getByLabelText("Message");
		const type = (field: HTMLElement, value: string) =>
			fireEvent.change(field, { target: { value } });
		expect(send().disabled).toBe(false);
		// A message with nothing in it is not sent.
		type(message, "   ");
		expect(send().disabled).toBe(true);
		// The most the daemon takes is 8,000 characters of the message and 200 of the subject.
		type(message, "m".repeat(8000));
		expect(send().disabled).toBe(false);
		type(message, "m".repeat(8001));
		expect(send().disabled).toBe(true);
		expect(within(dialog).getByText(en.sellerMessageLong)).toBeTruthy();
		type(message, "Hello");
		type(subject, "");
		expect(send().disabled).toBe(true);
		type(subject, "s".repeat(200));
		expect(send().disabled).toBe(false);
		type(subject, "s".repeat(201));
		expect(send().disabled).toBe(true);
		expect(within(dialog).getByText(en.sellerSubjectLong)).toBeTruthy();
	});

	it("close_sends_nothing", async () => {
		const { s, dialog } = await edited();
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		await vi.waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);
	});

	it("a_failed_send_keeps_the_dialog_open", async () => {
		const { s, dialog } = await edited({
			...KNOWN,
			why: "The server was busy",
			failed_at: FAILED_AT,
		});
		expect(within(dialog).getByRole("alert").textContent).toBe(
			"Catervas could not send it at 08:14: The server was busy. It is kept here to try again.",
		);
		fireEvent.click(within(dialog).getByRole("button", { name: "Send" }));
		const sent = await sentCommand(s);
		await s.reply(sent, {
			error: { kind: "failed", detail: "seller_message_failed: busy" },
		});
		const alerts = await within(dialog).findAllByRole("alert");
		expect(alerts.map((one) => one.textContent)).toContain(
			en.refuseSellerMessageFailed,
		);
		expect(screen.getByRole("dialog")).toBeTruthy();
	});
});
