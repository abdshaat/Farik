import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { showsWhatItHides } from "../test/hidden.ts";
import { EFFECTIVE, ORDERS, TEAM } from "../test/orders.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
import { at, MAILBOX, NO_MAILBOX } from "../test/sellerMail.ts";
import { NOW, SITES } from "../test/sites.ts";

/** One agent's page; the mailbox is asked for only where the role has one. */
async function opened(agent: string, mailbox: object) {
	vi.useFakeTimers({ toFake: ["Date"] });
	vi.setSystemTime(NOW);
	const { container, socket } = await renderApp(`/team/${agent}`);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", {
		team: TEAM,
		agents: EFFECTIVE,
		judges: { auto: null, architect: null, scrum_master: null },
		max_agents: 7,
		connectors: [],
		sandboxed: true,
	});
	await answerQuery(s, "models.list", { models: [] });
	await answerQuery(s, "skills.list", { skills: [] });
	if (agent === "ivo") {
		await answerQuery(s, "sites.list", SITES);
		await answerQuery(s, "purchase_orders.list", ORDERS);
		await answerQuery(s, "procurement_mailbox.get", mailbox);
	}
	await screen.findByRole("heading", {
		name:
			agent === "ivo"
				? "Ivo, your Procurement Specialist"
				: "Theo, your Developer",
	});
	return { container, s };
}

/** The section the page draws for the mailbox. */
const section = async () =>
	(await screen.findByRole("heading", { name: en.mailboxTitle })).closest(
		"section",
	) as HTMLElement;

describe("the procurement mailbox on the Procurement Specialist's page", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("shows_where_it_writes_from_and_when_it_was_checked", async () => {
		const { container } = await opened("ivo", MAILBOX);
		const mailbox = within(await section());
		expect(
			mailbox.getByText("Ivo writes from buying@cornerbakery.test."),
		).toBeTruthy();
		expect(
			mailbox.getByText(
				"Farik checks it for replies every 15 minutes. Last checked at 08:00.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("says_a_check_of_another_day_with_its_day", async () => {
		await opened("ivo", { ...MAILBOX, checked_at: at(25, 8) });
		expect(
			within(await section()).getByText(
				"Farik checks it for replies every 15 minutes. Last checked yesterday at 08:00.",
			),
		).toBeTruthy();
	});

	it("says_how_often_it_checks_before_the_first_check", async () => {
		const { checked_at: _checked, ...unchecked } = MAILBOX;
		await opened("ivo", unchecked);
		const mailbox = within(await section());
		expect(
			mailbox.getByText("Farik checks it for replies every 15 minutes."),
		).toBeTruthy();
		expect(mailbox.queryByText(/Last checked/)).toBeNull();
	});

	it("says_why_the_last_check_failed_in_the_page_s_own_words", async () => {
		await opened("ivo", {
			...MAILBOX,
			error: "Your provider‮ did not accept the sign-in",
			restarted_at: at(24, 9),
		});
		const mailbox = within(await section());
		// Farik's sentence, the time of the try, and what it holds that hides is written out.
		expect(
			mailbox.getByText(
				"Farik could not read it at 08:00: Your provider\\u{202e} did not accept the sign-in",
			),
		).toBeTruthy();
		showsWhatItHides(await section());
		expect(
			mailbox.getByText(/Your provider renumbered this mailbox 24 October/),
		).toBeTruthy();
	});

	it("check_now_checks_and_nothing_else", async () => {
		const { s } = await opened("ivo", MAILBOX);
		fireEvent.click(
			within(await section()).getByRole("button", { name: "Check now" }),
		);
		await waitFor(() =>
			expect(s.calls("procurement_mailbox.check")).toHaveLength(1),
		);
		expect(s.calls("procurement_mailbox.check")[0]?.params).toEqual({});
		expect(s.calls("procurement_mailbox.disconnect")).toHaveLength(0);
		expect(screen.queryByRole("dialog")).toBeNull();
	});

	it("disconnect_asks_first", async () => {
		const { s } = await opened("ivo", MAILBOX);
		fireEvent.click(
			within(await section()).getByRole("button", { name: "Disconnect" }),
		);
		// Nothing is forgotten until the owner says so in the question.
		const dialog = await screen.findByRole("dialog", {
			name: "Disconnect buying@cornerbakery.test?",
		});
		// On a phone the question fills the screen.
		expect(dialog.hasAttribute("data-fills-phone")).toBe(true);
		expect(
			within(dialog).getByText(
				"Farik forgets its app password and stops reading it. Messages and replies already kept stay in Ivo’s folder.",
			),
		).toBeTruthy();
		expect(s.calls("procurement_mailbox.disconnect")).toHaveLength(0);
		// Keeping it asks for nothing.
		fireEvent.click(within(dialog).getByRole("button", { name: "Keep it" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("procurement_mailbox.disconnect")).toHaveLength(0);
		// Only the question's own Disconnect forgets it.
		fireEvent.click(
			within(await section()).getByRole("button", { name: "Disconnect" }),
		);
		const again = await screen.findByRole("dialog", {
			name: "Disconnect buying@cornerbakery.test?",
		});
		fireEvent.click(within(again).getByRole("button", { name: "Disconnect" }));
		await waitFor(() =>
			expect(s.calls("procurement_mailbox.disconnect")).toHaveLength(1),
		);
		expect(s.calls("procurement_mailbox.disconnect")[0]?.params).toEqual({});
	});

	it("with_no_mailbox_it_says_so_and_links_to_the_page", async () => {
		await opened("ivo", NO_MAILBOX);
		const mailbox = within(await section());
		expect(
			mailbox.getByText(
				"No mailbox yet. Ivo can draft messages to sellers; you send them once a mailbox is connected.",
			),
		).toBeTruthy();
		expect(
			mailbox
				.getByRole("link", { name: en.mailboxConnectLink })
				.getAttribute("href"),
		).toBe("/team/ivo/mailbox");
		expect(mailbox.queryByRole("button", { name: "Check now" })).toBeNull();
	});

	it("only_the_procurement_specialist_has_one", async () => {
		const { s } = await opened("theo", MAILBOX);
		expect(screen.queryByRole("heading", { name: en.mailboxTitle })).toBeNull();
		expect(
			s
				.calls("query")
				.filter((q) => q.params.name === "procurement_mailbox.get"),
		).toHaveLength(0);
	});
});
