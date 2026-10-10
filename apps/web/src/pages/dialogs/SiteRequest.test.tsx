import { expectNoAxeViolations } from "@catervas/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { sentCommand } from "../../test/gate.ts";
import { todayWith } from "../../test/posts.ts";
import { SCRIPT_ROW, SITE_ROW, TEAM } from "../../test/sites.ts";

/** Today with both requests waiting; the row's own button for `kind` opened on `host`. */
async function opened(
	host: string,
	button: "Allow" | "Don’t allow",
	dialogName: string,
) {
	const { container, s } = await todayWith({
		waiting: [SITE_ROW, SCRIPT_ROW],
		team: TEAM,
	});
	const list = await screen.findByRole("list", { name: en.waitingList });
	const row = within(list)
		.getAllByRole("listitem")
		.find((one) => within(one).queryByText(host, { selector: "code" }));
	if (!row) throw new Error(`no row for ${host}`);
	fireEvent.click(within(row).getByRole("button", { name: button }));
	const dialog = await screen.findByRole("dialog", { name: dialogName });
	return { container, s: s as FakeSocket, dialog };
}

describe("a site request's dialog", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("allowing_and_refusing_a_site", async () => {
		const { container, s, dialog } = await opened(
			"pieboxpros.com",
			"Allow",
			"Allow Ivo to read pieboxpros.com?",
		);
		expect(
			within(dialog).getByText(/can then read any page on pieboxpros\.com/),
		).toBeTruthy();
		// A name in the Latin alphabet carries no warning.
		expect(within(dialog).queryByText(en.siteRequestScriptWhat)).toBeNull();
		await expectNoAxeViolations(container);

		// Closing sends nothing.
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);

		// Allowing without a note sends none, and a note is trimmed.
		fireEvent.click(
			within(
				(await screen.findAllByRole("listitem")).find((one) =>
					within(one).queryByText("pieboxpros.com", { selector: "code" }),
				) as HTMLElement,
			).getByRole("button", { name: "Allow" }),
		);
		const again = await screen.findByRole("dialog", {
			name: "Allow Ivo to read pieboxpros.com?",
		});
		fireEvent.click(within(again).getByRole("button", { name: "Allow" }));
		const allowed = await sentCommand(s);
		expect(allowed.params).toEqual({
			command: { command: "site_decide", body: { request: 61, allow: true } },
		});
		// A refusal is said in words, whatever the daemon's text is, and the dialog stays.
		await s.reply(allowed, {
			error: {
				kind: "refused",
				detail: "site_request_decided: request 61 was already decided",
			},
		});
		expect((await within(again).findByRole("alert")).textContent).toBe(
			en.refuseSiteRequestDecided,
		);
		fireEvent.change(within(again).getByLabelText(/A note for Ivo/), {
			target: { value: "  Ask them for a sample box before we order.  " },
		});
		fireEvent.click(within(again).getByRole("button", { name: "Allow" }));
		const withNote = await sentCommand(s, 2);
		expect(withNote.params).toEqual({
			command: {
				command: "site_decide",
				body: {
					request: 61,
					allow: true,
					note: "Ask them for a sample box before we order.",
				},
			},
		});
		await s.reply(withNote, { said: "allowed pieboxpros.com", events: [70] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("refusing_a_site_says_so_with_the_note", async () => {
		const { container, s, dialog } = await opened(
			"xn--ulne-m9d.com",
			"Don’t allow",
			"Don’t allow Ivo to read xn--ulne-m9d.com?",
		);
		// The name written in another alphabet is warned of in the dialog too.
		expect(within(dialog).getByText(en.siteRequestScriptWhat)).toBeTruthy();
		await expectNoAxeViolations(container);
		fireEvent.change(within(dialog).getByLabelText(/A note for Ivo/), {
			target: { value: "This is not Uline: one letter is not English." },
		});
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Don’t allow" }),
		);
		const refused = await sentCommand(s);
		expect(refused.params).toEqual({
			command: {
				command: "site_decide",
				body: {
					request: 62,
					allow: false,
					note: "This is not Uline: one letter is not English.",
				},
			},
		});
		await s.reply(refused, {
			said: "did not allow xn--ulne-m9d.com",
			events: [71],
		});
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});
});
