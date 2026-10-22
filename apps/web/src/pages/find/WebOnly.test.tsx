import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { answerCalls, IDLE, NOTHING } from "../../test/finding.ts";
import {
	answerQuery,
	renderApp,
	teamList,
	where,
} from "../../test/render-app.tsx";

const CATALOG = {
	services: [
		{ id: "zoho_books", name: "Zoho Books", kind: "bookkeeping" },
		{ id: "quickbooks", name: "QuickBooks", kind: "bookkeeping" },
		{ id: "microsoft_365", name: "Excel", kind: "spreadsheets" },
		{ id: "stripe", name: "Stripe", kind: "payments" },
	],
};

/** `fetch` as the page has it, with `POST /hub/export` answered by `answers`, one after another. */
function exportAnswers(answers: { status: number; body: object }[]) {
	const before = globalThis.fetch;
	const posted: { url: string; init: RequestInit }[] = [];
	const fetch = vi.fn(async (url: string, init?: RequestInit) => {
		if (url !== "/hub/export" || init?.method !== "POST")
			return before(url, init);
		posted.push({ url, init });
		const answer = answers[posted.length - 1] ?? { status: 500, body: {} };
		return new Response(JSON.stringify(answer.body), {
			status: answer.status,
			headers: { "Content-Type": "application/json" },
		});
	});
	vi.stubGlobal("fetch", fetch);
	return posted;
}

const zip = () =>
	new File(["PK"], "data-2026-10-20.zip", { type: "application/zip" });

describe("claude on the web, without GitHub", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
		localStorage.clear();
	});

	it("brings_in_claude_ai_s_export_and_the_services", async () => {
		const { hub, go } = await renderApp("/setup/where");
		const h = hub as FakeSocket;
		answerCalls(h, {
			"finding.status": { ...IDLE, phase: "done", index_exists: true },
			"finding.catalog": CATALOG,
			"finding.declare_services": {},
			"finding.export_dismissed": {},
			"finding.list": NOTHING,
		});

		// An owner who uses claude.ai goes to bring the history in.
		fireEvent.click(
			screen.getByRole("button", { name: /claude\.ai or the Claude app/ }),
		);
		await waitFor(() => expect(where()).toBe("/find/export"));
		expect(
			await screen.findByText("Bring your claude.ai history"),
		).toBeTruthy();
		expect(screen.getByText("1 of 3 · optional")).toBeTruthy();
		expect(screen.getByText("Click Export data.")).toBeTruthy();

		// Dropping the file posts it as the body, and says what came in.
		const posted = exportAnswers([
			{
				status: 200,
				body: {
					conversations: 12,
					new_conversations: 12,
					projects: 2,
					new_projects: 2,
					skipped: 0,
					skipped_projects: 0,
					content_at: "2026-10-20T00:00:00Z",
				},
			},
			{
				status: 422,
				body: { error: "not_an_export", message: "not an export" },
			},
		]);
		const zone = screen
			.getByText("Drop your export here")
			.closest("div") as HTMLElement;
		const file = zip();
		fireEvent.drop(zone, { dataTransfer: { files: [file] } });
		expect(
			await screen.findByText("12 conversations and 2 Projects brought in"),
		).toBeTruthy();
		expect(posted).toHaveLength(1);
		expect(posted[0]?.init.method).toBe("POST");
		expect(posted[0]?.init.body).toBe(file);

		// A file that is not claude.ai's is said so, with the way to another.
		fireEvent.drop(zone, { dataTransfer: { files: [zip()] } });
		expect(
			await screen.findByText("That isn't a claude.ai export"),
		).toBeTruthy();
		expect(
			screen.getByText(
				"Links work for 24 hours. Ask claude.ai for a new export.",
			),
		).toBeTruthy();

		// Later: nothing is called, and the card waits on All teams.
		fireEvent.click(screen.getByRole("button", { name: "I'll do it later" }));
		await waitFor(() => expect(where()).toBe("/find/services"));

		// Tapping the services declares them.
		const zoho = await screen.findByRole("button", { name: /Zoho Books/ });
		expect(zoho.getAttribute("aria-pressed")).toBe("false");
		fireEvent.click(zoho);
		fireEvent.click(screen.getByRole("button", { name: /Excel/ }));
		expect(zoho.getAttribute("aria-pressed")).toBe("true");
		expect(
			screen.getByText("Tap any. Bookkeeping becomes a project of its own."),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Continue" }));
		const declared = await waitFor(() => {
			const frame = h.calls("finding.declare_services")[0];
			if (!frame) throw new Error("no finding.declare_services was sent");
			return frame;
		});
		expect(declared.params).toEqual({ ids: ["zoho_books", "microsoft_365"] });
		await waitFor(() => expect(where()).toBe("/find/found"));

		// The card that waits for the export is on All teams, and Dismiss ends it.
		go("/teams");
		await answerQuery(h, "teams.list", teamList("shop"));
		const card = (
			await screen.findByText("Waiting for your claude.ai export.")
		).closest("section") as HTMLElement;
		expect(
			within(card).getByText("Drop it here when the email comes."),
		).toBeTruthy();
		expect(
			within(card).getByRole("button", { name: "Choose file…" }),
		).toBeTruthy();
		fireEvent.click(within(card).getByRole("button", { name: "Dismiss" }));
		await waitFor(() =>
			expect(h.calls("finding.export_dismissed")).toHaveLength(1),
		);
		await waitFor(() =>
			expect(
				screen.queryByText("Waiting for your claude.ai export."),
			).toBeNull(),
		);
	});

	it("sends_an_owner_with_nothing_found_to_claude_ai", async () => {
		const { hub } = await renderApp("/find/found");
		const h = hub as FakeSocket;
		answerCalls(h, {
			"finding.status": { ...IDLE, phase: "done", index_exists: true },
			"finding.list": NOTHING,
		});
		fireEvent.click(
			await screen.findByRole("button", { name: "I use claude.ai" }),
		);
		await waitFor(() => expect(where()).toBe("/find/export"));
	});
});
