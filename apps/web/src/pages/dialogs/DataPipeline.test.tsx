import { expectNoAxeViolations } from "@catervas/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import { sentCommand } from "../../test/gate.ts";
import { HIDDEN, PAID_ROW, TEAM } from "../../test/pipelines.ts";
import { todayWith } from "../../test/posts.ts";
import { refusedBy } from "../../test/schema.ts";

/** Today with the request waiting; its row's own button opened. */
async function opened(button: "Approve" | "Decline", dialogName: string) {
	const { container, s } = await todayWith({ waiting: [PAID_ROW], team: TEAM });
	const list = await screen.findByRole("list", { name: en.waitingList });
	fireEvent.click(
		within(within(list).getByRole("listitem")).getByRole("button", {
			name: button,
		}),
	);
	const dialog = await screen.findByRole("dialog", { name: dialogName });
	return { container, s, dialog };
}

/** The body of the `data_pipeline_decide` a request carries. */
function bodyOf(sent: { params: unknown }): Record<string, unknown> {
	const params = sent.params as { command: { body: Record<string, unknown> } };
	return params.command.body;
}

describe("a data pipeline's dialog", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("approve_shows_the_filed_text_then_sends_approve_with_the_note", async () => {
		const { container, s, dialog } = await opened(
			"Approve",
			"Approve Firecrawl for Ivo?",
		);
		expect(
			within(dialog).getByText(
				"The team gets this request in your name. Ivo wrote most of it: read it first.",
			),
		).toBeTruthy();
		// The whole text the team will get, framed as written by others, as text.
		const filed = within(dialog).getByRole("group", { name: "The request" });
		expect(filed.getAttribute("data-trust")).toBe("untrusted");
		expect(filed.textContent).toBe(
			PAID_ROW.request_text.replace(HIDDEN, "\\u{202e}"),
		);
		await expectNoAxeViolations(container);

		// Closing decides nothing.
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);

		// With no note none is sent.
		fireEvent.click(screen.getByRole("button", { name: "Approve" }));
		const again = await screen.findByRole("dialog", {
			name: "Approve Firecrawl for Ivo?",
		});
		fireEvent.change(within(again).getByLabelText(/A note for Ivo/), {
			target: { value: "   " },
		});
		fireEvent.click(within(again).getByRole("button", { name: "Approve" }));
		const plain = await sentCommand(s);
		expect(plain.params).toEqual({
			command: {
				command: "data_pipeline_decide",
				body: { pipeline: 7, decision: "approve" },
			},
		});
		expect(refusedBy("dataPipelineDecideBody", bodyOf(plain))).toEqual([]);
		await s.reply(plain, {
			error: {
				kind: "refused",
				detail: "pipeline_decided: request 7 was decided already",
			},
		});
		expect((await within(again).findByRole("alert")).textContent).toBe(
			en.refusePipelineDecided,
		);
		// A note is trimmed.
		fireEvent.change(within(again).getByLabelText(/A note for Ivo/), {
			target: { value: "  Use it for those two sellers this month.  " },
		});
		fireEvent.click(within(again).getByRole("button", { name: "Approve" }));
		const withNote = await sentCommand(s, 2);
		expect(withNote.params).toEqual({
			command: {
				command: "data_pipeline_decide",
				body: {
					pipeline: 7,
					decision: "approve",
					note: "Use it for those two sellers this month.",
				},
			},
		});
		expect(refusedBy("dataPipelineDecideBody", bodyOf(withNote))).toEqual([]);
		await s.reply(withNote, { said: "approved Firecrawl", events: [80] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("decline_sends_decline_with_the_note", async () => {
		const { container, s, dialog } = await opened(
			"Decline",
			"Decline Firecrawl for Ivo?",
		);
		expect(
			within(dialog).getByText(
				"Ivo reads your note in its next piece of work and goes on without it.",
			),
		).toBeTruthy();
		expect(
			within(dialog).queryByRole("group", { name: "The request" }),
		).toBeNull();
		await expectNoAxeViolations(container);
		fireEvent.change(within(dialog).getByLabelText(/A note for Ivo/), {
			target: { value: "Keep to the three sellers you can read." },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Decline" }));
		const declined = await sentCommand(s);
		expect(declined.params).toEqual({
			command: {
				command: "data_pipeline_decide",
				body: {
					pipeline: 7,
					decision: "decline",
					note: "Keep to the three sellers you can read.",
				},
			},
		});
		expect(refusedBy("dataPipelineDecideBody", bodyOf(declined))).toEqual([]);
		await s.reply(declined, { said: "declined Firecrawl", events: [81] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("close_decides_nothing", async () => {
		const { s, dialog } = await opened("Decline", "Decline Firecrawl for Ivo?");
		fireEvent.change(within(dialog).getByLabelText(/A note for Ivo/), {
			target: { value: "never sent" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);
	});

	it("a_refusal_shows_its_words", async () => {
		const { s, dialog } = await opened("Decline", "Decline Firecrawl for Ivo?");
		const words: [string, string][] = [
			["pipeline_not_escalated", en.refusePipelineNotEscalated],
			["unknown_pipeline", en.refusePipelineUnknown],
			["pipeline_note_too_long", en.refusePipelineNoteLong],
		];
		let sent = 0;
		for (const [code, said] of words) {
			fireEvent.click(within(dialog).getByRole("button", { name: "Decline" }));
			sent += 1;
			const call = await sentCommand(s, sent);
			await s.reply(call, {
				error: { kind: "refused", detail: `${code}: whatever the daemon says` },
			});
			await waitFor(() =>
				expect(within(dialog).getByRole("alert").textContent).toBe(said),
			);
		}
	});
});
