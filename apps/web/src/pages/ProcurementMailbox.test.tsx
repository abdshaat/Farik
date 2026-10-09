import { expectNoAxeViolations } from "@farik/ui/test";
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
	const { container, socket } = await renderApp("/team/ivo/mailbox");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "procurement_mailbox.get", NO_MAILBOX);
	await screen.findByRole("heading", { name: en.mailboxPageTitle });
	return { container, s };
}

const type = (label: RegExp | string, value: string) =>
	fireEvent.change(screen.getByLabelText(label), { target: { value } });

/** The value of the checked choice in the group `name`. */
const chosen = (name: string) =>
	(document.querySelector(`input[name="${name}"]:checked`) as HTMLInputElement)
		?.value;

const field = (id: string) =>
	document.querySelector(`#${id}`) as HTMLInputElement;

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

	it("a_known_provider_has_its_help_its_locked_servers_and_sends_nothing", async () => {
		const { container } = await page();
		// Gmail: the steps, the help it names, the servers it fixes, and what connecting does.
		const help = screen.getByRole("link", {
			name: "How Gmail app passwords work",
		});
		expect(help.getAttribute("href")).toBe(
			"https://support.google.com/accounts/answer/185833",
		);
		expect(field("mailbox-imap-host").matches(":disabled")).toBe(true);
		expect(field("mailbox-smtp-port").matches(":disabled")).toBe(true);
		expect(screen.getByText(en.mailboxSendsNothing)).toBeTruthy();
		await expectNoAxeViolations(container);
		// iCloud sends over STARTTLS, which the page shows and does not let the owner change.
		fireEvent.click(screen.getByLabelText("iCloud Mail"));
		expect(
			screen
				.getByRole("link", { name: "How iCloud app passwords work" })
				.getAttribute("href"),
		).toBe("https://support.apple.com/102654");
		expect(chosen("mailbox-smtp-security")).toBe("starttls");
		expect(chosen("mailbox-imap-security")).toBe("tls");
		// Fastmail names its plan that cannot be used.
		fireEvent.click(screen.getByLabelText("Fastmail"));
		expect(screen.getByText(en.mailboxHowFastmail)).toBeTruthy();
		expect(
			screen
				.getByRole("link", { name: "How Fastmail app passwords work" })
				.getAttribute("href"),
		).toBe("https://www.fastmail.help/hc/articles/360058752854");
		// Another provider's servers are typed.
		fireEvent.click(screen.getByLabelText("Another provider"));
		expect(field("mailbox-imap-host").matches(":disabled")).toBe(false);
		expect(
			screen.queryByRole("link", { name: /app passwords work/ }),
		).toBeNull();
	});

	it("the_provider_is_found_from_the_address_s_domain", async () => {
		const { container } = await page();
		type("The address", "buying@icloud.com");
		expect(chosen("mailbox-provider")).toBe("icloud");
		expect(field("mailbox-smtp-host").value).toBe("smtp.mail.me.com");
		type("The address", "buying@googlemail.com");
		expect(chosen("mailbox-provider")).toBe("gmail");
		type("The address", "buying@fastmail.fm");
		expect(chosen("mailbox-provider")).toBe("fastmail");
		// A Microsoft address chooses Microsoft, which says it is not supported and offers no Connect.
		type("The address", "buying@Outlook.com");
		expect(chosen("mailbox-provider")).toBe("microsoft");
		expect(screen.getByText(en.mailboxMicrosoft)).toBeTruthy();
		expect(screen.queryByRole("button", { name: "Connect" })).toBeNull();
		await expectNoAxeViolations(container);
		// An address at a domain it does not know leaves the choice as it is.
		fireEvent.click(screen.getByLabelText("Another provider"));
		type("The address", "buying@cornerbakery.test");
		expect(chosen("mailbox-provider")).toBe("other");
	});

	it("another_provider_needs_its_servers_before_it_connects", async () => {
		const { s } = await page();
		fireEvent.click(screen.getByLabelText("Another provider"));
		type("The address", "buying@cornerbakery.test");
		type("Your name, as sellers see it", "Sam Ortiz");
		type("App password", "pw-from-the-owner");
		const connect = () =>
			screen.getByRole("button", { name: "Connect" }) as HTMLButtonElement;
		// Servers that are not named are not sent, with a port of 0 that the daemon would refuse.
		expect(connect().disabled).toBe(true);
		fireEvent.change(field("mailbox-imap-host"), {
			target: { value: "imap.cornerbakery.test" },
		});
		fireEvent.change(field("mailbox-smtp-host"), {
			target: { value: "smtp.cornerbakery.test" },
		});
		expect(connect().disabled).toBe(true);
		// Each server needs a port that is a number from 1 to 65535, the one as the other.
		fireEvent.change(field("mailbox-imap-port"), { target: { value: "993" } });
		expect(connect().disabled).toBe(true);
		for (const port of ["0", "65536", "imap", "9 3", "1e3", "9.5"]) {
			fireEvent.change(field("mailbox-smtp-port"), { target: { value: port } });
			expect(connect().disabled).toBe(true);
		}
		fireEvent.change(field("mailbox-imap-port"), { target: { value: "" } });
		fireEvent.change(field("mailbox-smtp-port"), { target: { value: "587" } });
		expect(connect().disabled).toBe(true);
		fireEvent.change(field("mailbox-imap-port"), { target: { value: "993" } });
		fireEvent.change(field("mailbox-smtp-host"), { target: { value: " " } });
		expect(connect().disabled).toBe(true);
		fireEvent.change(field("mailbox-smtp-host"), {
			target: { value: "smtp.cornerbakery.test" },
		});
		fireEvent.click(
			screen.getByLabelText("Encrypted after connecting (STARTTLS)", {
				selector: "[name='mailbox-smtp-security']",
			}),
		);
		expect(connect().disabled).toBe(false);
		fireEvent.click(connect());
		await waitFor(() =>
			expect(s.calls("procurement_mailbox.connect")).toHaveLength(1),
		);
		const params = (
			s.calls("procurement_mailbox.connect")[0] as never as {
				params: Record<string, unknown>;
			}
		).params;
		expect(params.imap).toEqual({
			host: "imap.cornerbakery.test",
			port: 993,
			security: "tls",
		});
		expect(params.smtp).toEqual({
			host: "smtp.cornerbakery.test",
			port: 587,
			security: "starttls",
		});
		expect(refusedBy("procurementMailboxConnectRequest", params)).toEqual([]);
	});

	it("a_refused_connect_is_said_in_words_a_person_can_act_on", async () => {
		const { s } = await page();
		type("The address", "buying@cornerbakery.test");
		type("Your name, as sellers see it", "Sam Ortiz");
		type("App password", "pw-from-the-owner");
		fireEvent.click(screen.getByRole("button", { name: "Connect" }));
		await waitFor(() =>
			expect(s.calls("procurement_mailbox.connect")).toHaveLength(1),
		);
		// Params the daemon's schema refuses carry no code and no data.
		await s.fail(
			s.calls("procurement_mailbox.connect")[0] as never,
			-32602,
			"invalid params",
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseMailboxSettings,
		);
		// So do the daemon's own, by their code.
		fireEvent.change(screen.getByLabelText("App password"), {
			target: { value: "pw-again" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Connect" }));
		await waitFor(() =>
			expect(s.calls("procurement_mailbox.connect")).toHaveLength(2),
		);
		await s.fail(
			s.calls("procurement_mailbox.connect")[1] as never,
			-32000,
			"secret_store_unavailable: this computer has no keychain",
		);
		await waitFor(() =>
			expect(screen.getByRole("alert").textContent).toBe(en.refuseSecretStore),
		);
	});

	it("microsoft_is_not_supported_yet", async () => {
		const { container } = await page();
		fireEvent.click(screen.getByLabelText(/Outlook.com or Microsoft 365/));
		expect(screen.getByText(en.mailboxMicrosoft)).toBeTruthy();
		expect(screen.queryByRole("button", { name: "Connect" })).toBeNull();
		expect(screen.getByText(/Not supported yet/)).toBeTruthy();
		await expectNoAxeViolations(container);
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
		const { s } = await page();
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
		// The password was sent the once: a refusal does not send it again.
		expect(s.calls("procurement_mailbox.connect")).toHaveLength(1);
	});
});
