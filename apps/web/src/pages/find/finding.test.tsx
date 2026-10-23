import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { answerCalls, IDLE } from "../../test/finding.ts";
import { answerQuery, renderApp, where } from "../../test/render-app.tsx";

const READY = { state: "ready", version: "2.1.300" };
const CHECK = {
	claude: READY,
	git: READY,
	docker: { state: "ready" },
	sandbox_image: { state: "ready" },
};

describe("finding projects", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
		sessionStorage.clear();
		localStorage.clear();
	});

	it("offers_to_find_projects_and_shows_the_scan", async () => {
		const { hub } = await renderApp("/setup/computer");
		const h = hub as FakeSocket;
		const table: Record<string, unknown> = {
			"finding.offer": { history: "claude_code", sessions_dir_exists: true },
			"finding.status": IDLE,
			"finding.start": { started: true },
			"finding.stop": {},
		};
		answerCalls(h, table);
		await answerQuery(h, "computer.check", CHECK);

		// The computer check offers it, with the words of the mockup.
		expect(await screen.findByText("Find your projects?")).toBeTruthy();
		expect(screen.getByText("We found your Claude Code history")).toBeTruthy();
		expect(screen.getByText("Nothing leaves this computer.")).toBeTruthy();
		table["finding.status"] = {
			...IDLE,
			phase: "running",
			projects: 7,
			jobs: 4,
			sessions: 312,
			reading: "sessions from August",
			seconds_left: 60,
		};
		fireEvent.click(
			screen.getByRole("button", { name: "Look for my projects" }),
		);
		await waitFor(() => expect(h.calls("finding.start")).toHaveLength(1));
		await waitFor(() => expect(where()).toBe("/find/scanning"));

		// The counts, the time left, and what is being read.
		expect(await screen.findByText("7 projects")).toBeTruthy();
		expect(screen.getByText("4 scheduled jobs")).toBeTruthy();
		expect(screen.getByText("312 sessions")).toBeTruthy();
		expect(screen.getByText("About a minute left.")).toBeTruthy();
		expect(screen.getByText("Reading sessions from August")).toBeTruthy();
		expect(screen.getByText("What's found so far is kept.")).toBeTruthy();

		// Setup goes on meanwhile, with the strip on its pages.
		fireEvent.click(
			screen.getByRole("button", { name: "Continue setup meanwhile" }),
		);
		await waitFor(() => expect(where()).toBe("/setup/computer"));
		expect(
			await screen.findByText("Still looking · 7 projects so far"),
		).toBeTruthy();
		expect(
			screen
				.getByRole("link", { name: "See what's found" })
				.getAttribute("href"),
		).toBe("/find/found");
	});

	it("shows_the_scan_ending_and_stopping", async () => {
		const { hub } = await renderApp("/find/scanning");
		const h = hub as FakeSocket;
		const table: Record<string, unknown> = {
			"finding.status": { ...IDLE, phase: "running", projects: 2 },
			"finding.stop": {},
		};
		answerCalls(h, table);
		await screen.findByText("2 projects");
		fireEvent.click(screen.getByRole("button", { name: "Stop" }));
		await waitFor(() => expect(h.calls("finding.stop")).toHaveLength(1));

		// It ended: the page says so, and leads on.
		table["finding.status"] = {
			...IDLE,
			phase: "done",
			projects: 7,
			jobs: 4,
			sessions: 312,
			unreadable: 41,
		};
		expect(
			await screen.findByText("Here's what we found.", {}, { timeout: 3000 }),
		).toBeTruthy();
		// Sessions of a newer Claude Code are said, with no update to press.
		expect(screen.getByText("Couldn't read 41 sessions")).toBeTruthy();
		expect(screen.getByText("Everything else was found.")).toBeTruthy();
		expect(screen.queryByRole("button", { name: /update/i })).toBeNull();
		expect(
			screen.getByRole("button", { name: "See what you work on" }),
		).toBeTruthy();
	});

	it("goes_to_where_you_use_claude_without_history", async () => {
		const { hub } = await renderApp("/setup/computer");
		const h = hub as FakeSocket;
		answerCalls(h, {
			"finding.offer": { history: "none", sessions_dir_exists: false },
			"finding.status": IDLE,
		});
		await answerQuery(h, "computer.check", CHECK);
		const next = await screen.findByRole("button", { name: "Continue" });
		await waitFor(() =>
			expect((next as HTMLButtonElement).disabled).toBe(false),
		);
		// Without history there is no offer to find projects.
		expect(screen.queryByText("Find your projects?")).toBeNull();
		fireEvent.click(next);
		await waitFor(() => expect(where()).toBe("/setup/where"));
	});
});
