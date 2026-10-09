import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
import { refusedBy } from "../test/schema.ts";
import { NO_MAILBOX, TEAM } from "../test/sellerMail.ts";

afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
});

async function page() {
	const { socket } = await renderApp("/team/ivo/mailbox");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "procurement_mailbox.get", NO_MAILBOX);
	await screen.findByRole("heading", { name: en.mailboxPageTitle });
	return s;
}

const type = (label: RegExp | string, value: string) =>
	fireEvent.change(screen.getByLabelText(label), { target: { value } });

describe("the procurement mailbox page", () => {
	it("the_mailbox_page_fills_a_known_provider_s_servers", async () => {
		await page();
		// Gmail is chosen: its servers are filled in and not to be typed.
		expect(
			(
				screen.getByLabelText("Server", {
					selector: "#mailbox-imap-host",
				}) as HTMLInputElement
			).value,
		).toBe("imap.gmail.com");
		expect(
			(document.querySelector("#mailbox-smtp-host") as HTMLInputElement).value,
		).toBe("smtp.gmail.com");
		expect(
			(document.querySelector("#mailbox-smtp-port") as HTMLInputElement).value,
		).toBe("465");
		expect(screen.getByText(en.mailboxHowGmail1)).toBeTruthy();
		fireEvent.click(screen.getByLabelText("iCloud Mail"));
		expect(
			(document.querySelector("#mailbox-smtp-host") as HTMLInputElement).value,
		).toBe("smtp.mail.me.com");
		expect(
			(document.querySelector("#mailbox-smtp-port") as HTMLInputElement).value,
		).toBe("587");
		fireEvent.click(screen.getByLabelText("Another provider"));
		expect(
			(document.querySelector("#mailbox-imap-host") as HTMLInputElement).value,
		).toBe("");
		expect(screen.getByText(en.mailboxHowOther)).toBeTruthy();
	});

	it("microsoft_is_not_supported_yet", async () => {
		await page();
		fireEvent.click(screen.getByLabelText(/Outlook.com or Microsoft 365/));
		expect(screen.getByText(en.mailboxMicrosoft)).toBeTruthy();
		expect(screen.queryByRole("button", { name: "Connect" })).toBeNull();
		expect(screen.getByText(/Not supported yet/)).toBeTruthy();
	});

	it("the_alias_note_is_always_shown", async () => {
		await page();
		expect(
			screen.getByText(
				/If it is an alias, its sign-in reaches your whole mailbox/,
			),
		).toBeTruthy();
		fireEvent.click(screen.getByLabelText("Fastmail"));
		expect(
			screen.getByText(/marks nothing read, moves nothing and deletes nothing/),
		).toBeTruthy();
		fireEvent.click(screen.getByLabelText(/Outlook.com or Microsoft 365/));
		expect(screen.getByText(/If it is an alias/)).toBeTruthy();
	});

	it("connect_sends_the_password_once_and_clears_it", async () => {
		const s = await page();
		type("The address", "buying@cornerbakery.test");
		type("Your name, as sellers see it", "Sam Ortiz");
		type("App password", "pw-from-the-owner");
		fireEvent.click(screen.getByRole("button", { name: "Connect" }));
		await waitFor(() =>
			expect(s.calls("procurement_mailbox.connect")).toHaveLength(1),
		);
		const frame = s.calls("procurement_mailbox.connect")[0] as never;
		expect((frame as { params: unknown }).params).toEqual({
			address: "buying@cornerbakery.test",
			name: "Sam Ortiz",
			provider: "gmail",
			imap: { host: "imap.gmail.com", port: 993, security: "tls" },
			smtp: { host: "smtp.gmail.com", port: 465, security: "tls" },
			username: "buying@cornerbakery.test",
			password: "pw-from-the-owner",
			folder: "INBOX",
			signature: "",
			disclose_ai: true,
		});
		// What the daemon reads: the schema of the method's params, nested servers included.
		expect(
			refusedBy(
				"procurementMailboxConnectRequest",
				(frame as { params: Record<string, unknown> }).params,
			),
		).toEqual([]);
		// A refusal is said in Farik's words and the password is gone from the field.
		await s.fail(
			frame,
			-32000,
			"mailbox_login_failed: the provider said no (535)",
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseMailboxLoginFailed,
		);
		expect(
			(screen.getByLabelText("App password") as HTMLInputElement).value,
		).toBe("");
		expect(document.body.textContent).not.toContain("pw-from-the-owner");
		expect(document.body.textContent).not.toContain("535");
	});
});
