import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { sentCommand } from "../../test/gate.ts";
import {
	answerQuery,
	answerStatus,
	renderApp,
} from "../../test/render-app.tsx";

const TEAM = {
	name: "Corner Bakery",
	agents: [
		{
			id: "theo",
			display_name: "Theo",
			role: "software_developer",
			avatar: "developer",
			status: "active",
		},
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};

/** Today with one call waiting to be allowed, its dialog open. */
async function opened(input: string) {
	const { container, socket } = await renderApp("/");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "team.activity", { activity: [] });
	await answerQuery(s, "waiting.list", {
		waiting: [
			{
				task_id: "FRK-14",
				kind: "tool_approval",
				agent_id: "theo",
				title: "Sold-out badge on the menu",
				line: "Theo wants to use airtable",
				approval: 31,
				server: "airtable",
				tool: "create_record",
				input,
			},
		],
	});
	fireEvent.click(
		await screen.findByRole("button", { name: en.waitingReview }),
	);
	const dialog = await screen.findByRole("dialog", {
		name: "Theo wants to use airtable",
	});
	return { container, s, dialog };
}

describe("tool approval", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("the_dialog_sends_approve_or_refuse_with_the_note", async () => {
		const { container, s, dialog } = await opened('{"table":"Menu items"}');
		await expectNoAxeViolations(container);
		fireEvent.change(within(dialog).getByLabelText(/A note for Theo/), {
			target: { value: "Only this once." },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Allow once" }));
		const allowed = await sentCommand(s);
		expect(allowed.params).toEqual({
			command: {
				command: "tool_approve",
				body: { approval: 31, note: "Only this once." },
			},
		});
		await s.reply(allowed, { said: "Allowed", events: [32] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

		// Without a note, none is sent.
		fireEvent.click(screen.getByRole("button", { name: en.waitingReview }));
		const again = await screen.findByRole("dialog", {
			name: "Theo wants to use airtable",
		});
		fireEvent.click(within(again).getByRole("button", { name: "Don’t allow" }));
		expect((await sentCommand(s, 2)).params).toEqual({
			command: { command: "tool_refuse", body: { approval: 31 } },
		});
	});

	it("the_input_is_shown_as_untrusted_text", async () => {
		const long = `${"word ".repeat(2000)}END`;
		const input = JSON.stringify({
			title: "<img src=x onerror=alert(1)><b>Sold out</b>",
			body: long,
		});
		const { container, dialog } = await opened(input);
		const frame = within(dialog).getByRole("region", {
			name: "What Theo wants to send",
		});
		expect(frame.getAttribute("data-trust")).toBe("untrusted");
		expect(frame.textContent).toContain(
			'"title": "<img src=x onerror=alert(1)><b>Sold out</b>"',
		);
		expect(frame.textContent).toContain(long);
		expect(
			container.ownerDocument.querySelector("dialog img, dialog b"),
		).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("an_input_that_would_change_is_shown_as_it_came", async () => {
		const raw = '{"amount":12345678901234567890,"a":1,"a":2}';
		const { dialog } = await opened(raw);
		expect(
			within(dialog).getByRole("region", { name: "What Theo wants to send" })
				.textContent,
		).toBe(raw);
	});

	it("characters_that_hide_or_reorder_text_are_shown_as_markers", async () => {
		// A right-to-left override and a zero-width space: what the human reads must be what
		// is sent, so each shows as its code point.
		const input = JSON.stringify({ to: "a\u202Eb\u200Bc", amount: "100" });
		const { container, dialog } = await opened(input);
		const text = within(dialog).getByRole("region", {
			name: "What Theo wants to send",
		}).textContent;
		expect(text).toContain("a\\u{202e}b\\u{200b}c");
		expect(text).not.toMatch(/[\u202e\u200b]/);
		await expectNoAxeViolations(container);
	});

	it("says_the_fields_are_in_alphabetical_order", async () => {
		const { dialog } = await opened('{"a":2,"b":1}');
		expect(within(dialog).getByText(/alphabetical order/)).toBeTruthy();
	});
});
