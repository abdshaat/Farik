import { expectNoAxeViolations } from "@catervas/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { sentCommand } from "../../test/gate.ts";
import { showsWhatItHides } from "../../test/hidden.ts";
import { bodyOf, refusedBy } from "../../test/schema.ts";
import {
	MARKUP,
	MESSAGE,
	REPLY,
	todayWithMail,
	WRITTEN_REPLY,
} from "../../test/sellerMail.ts";
import styles from "../pages.module.css";

afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
	vi.restoreAllMocks();
});

async function read() {
	const { container, s } = await todayWithMail({
		messages: [MESSAGE],
		replies: [REPLY],
	});
	fireEvent.click(await screen.findByRole("button", { name: "Read" }));
	const dialog = await screen.findByRole("dialog", {
		name: "Reply from Packaging Express",
	});
	return { container, s, dialog };
}

describe("a reply from a seller, read", () => {
	it("the_reply_is_text_and_links_are_not_links", async () => {
		const { dialog } = await read();
		const frame = within(dialog).getByText(/Pay at https:\/\/evil.test\/pay/);
		const fieldset = frame.closest("fieldset") as HTMLElement;
		expect(fieldset.getAttribute("data-trust")).toBe("untrusted");
		// The frame wraps long lines, scrolls and is drawn as a frame, as every untrusted text is,
		// and holds the reply's subject with its words.
		expect(fieldset.className).toBe(styles.untrusted);
		expect(within(fieldset).getByText(`Re: Quote ${MARKUP}`)).toBeTruthy();
		// The address and the link are words, and the markup is shown as typed.
		expect(frame.textContent).toContain("pay@evil.test");
		expect(frame.textContent).toContain(MARKUP);
		expect(dialog.querySelector("fieldset a")).toBeNull();
		expect(dialog.querySelector("fieldset b")).toBeNull();
		expect(
			within(dialog).getByText(/Anyone can write any From address/),
		).toBeTruthy();
		expect(
			within(dialog).getByText(/never as instructions to follow/),
		).toBeTruthy();
		// A kept file offers a download; one that was not kept says so.
		expect(
			within(dialog).getByText(/PDF, 81 KB, named “quote.pdf”/),
		).toBeTruthy();
		expect(
			within(dialog).getByText(
				"“tool.exe” was not kept: Catervas keeps only PDFs and pictures of 10 MB or less.",
			),
		).toBeTruthy();
	});

	it("the_reply_shows_what_the_seller_hid", async () => {
		const { s } = await todayWithMail({
			messages: [MESSAGE],
			replies: [WRITTEN_REPLY],
		});
		fireEvent.click(await screen.findByRole("button", { name: "Read" }));
		// The title names the seller with the hidden character written out.
		const dialog = await screen.findByRole("dialog", {
			name: "Reply from Packaging\\u{202e} Express",
		});
		// From, the subject Catervas sent, the reply's subject and words, and both file names.
		showsWhatItHides(dialog);
		expect(s.calls("command")).toHaveLength(0);
	});

	it("the_reply_dialog_fills_a_phone_and_says_when_it_came", async () => {
		const { container, dialog } = await read();
		// On a phone the dialog fills the screen, so a long reply is read on the whole of it.
		expect(dialog.hasAttribute("data-fills-phone")).toBe(true);
		expect(within(dialog).getByText("Received today at 09:00")).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("ask_for_a_comparison_files_then_dismisses", async () => {
		const { s, dialog } = await read();
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Ask for a comparison" }),
		);
		const asking = await screen.findByRole("dialog", {
			name: "Ask Ivo to compare the replies?",
		});
		const text = within(asking).getByLabelText(
			/Your request/,
		) as HTMLTextAreaElement;
		// The draft is the owner's words and quotes nothing the seller wrote.
		expect(text.value).toBe(
			"Compare the sellers’ replies for CTV-31 Find a supplier for 500 pie boxes, and tell me which offer is best.",
		);
		fireEvent.click(
			within(asking).getByRole("button", { name: "Send to the team" }),
		);
		await waitFor(() => expect(s.calls("request.file")).toHaveLength(1));
		await s.reply(s.calls("request.file")[0] as never, { task_id: "CTV-40" });
		const dismissed = await sentCommand(s);
		expect(dismissed.params).toEqual({
			command: { command: "seller_reply_dismiss", body: { reply: 7 } },
		});
		expect(refusedBy("sellerReplyDismissBody", bodyOf(dismissed))).toEqual([]);
	});

	it("a_refused_request_dismisses_nothing", async () => {
		const { s, dialog } = await read();
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Ask for a comparison" }),
		);
		const asking = await screen.findByRole("dialog", {
			name: "Ask Ivo to compare the replies?",
		});
		fireEvent.click(
			within(asking).getByRole("button", { name: "Send to the team" }),
		);
		await waitFor(() => expect(s.calls("request.file")).toHaveLength(1));
		await s.fail(s.calls("request.file")[0] as never, -32000, "refused", {
			errors: [{ path: "", message: "too short", code: "too_short" }],
		});
		await within(asking).findByRole("alert");
		expect(s.calls("command")).toHaveLength(0);
	});

	it("downloads_a_kept_attachment", async () => {
		const { s, dialog } = await read();
		URL.createObjectURL = vi.fn(() => "blob:x");
		URL.revokeObjectURL = vi.fn();
		const clicked = vi
			.spyOn(HTMLAnchorElement.prototype, "click")
			.mockImplementation(() => {});
		// Each file's button says which file it fetches, so two of them are told apart.
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Download quote.pdf" }),
		);
		await waitFor(() =>
			expect(s.calls("seller_reply.attachment")).toHaveLength(1),
		);
		const frame = s.calls("seller_reply.attachment")[0] as never;
		expect((frame as { params: unknown }).params).toEqual({
			reply: 7,
			index: 1,
		});
		await s.reply(frame, {
			media_type: "application/pdf",
			base64: btoa("%PDF"),
			name: "1-1.pdf",
		});
		await waitFor(() => expect(clicked).toHaveBeenCalled());
	});
});
