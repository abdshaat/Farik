import { fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import { sentCommand } from "../../test/gate.ts";
import { bodyOf, refusedBy } from "../../test/schema.ts";
import { KNOWN, todayWithMail } from "../../test/sellerMail.ts";

afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

async function edited(message: object = KNOWN) {
	const { s } = await todayWithMail({ messages: [message] });
	fireEvent.click(await screen.findByRole("button", { name: "Edit" }));
	const dialog = await screen.findByRole("dialog", {
		name: "Edit the message to Pie Box Pros",
	});
	return { s, dialog };
}

describe("a message to a seller, edited", () => {
	it("edit_then_send_sends_the_edited_text", async () => {
		const { s, dialog } = await edited();
		// From and To stay as the agent wrote them, with what Farik adds.
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
		});
		expect(within(dialog).getByRole("alert").textContent).toBe(
			"Farik could not send it: The server was busy. It is kept here to try again.",
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
