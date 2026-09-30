import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	status: "active",
});
const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		agent("sol", "Sol", "scrum_master", "scrum-master"),
		agent("theo", "Theo", "software_developer", "developer"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};
const SPRINT = {
	sprint_id: "S2",
	status: "open",
	started_at: "2026-09-21T09:00:00Z",
	started_by: "human",
	ended_at: null,
	budget_usd: 20,
	spent_usd: 11.84,
	planned_by: "sol",
	task_count: 3,
	done_count: 1,
	tasks: [
		{ task_id: "FRK-14", title: "New checkout page", status: "verifying" },
		{ task_id: "FRK-15", title: "Show sold-out items", status: "rejected" },
		{ task_id: "FRK-12", title: "New photos", status: "accepted" },
	],
	meetings: [
		{ thread: "planning", first_seq: 40, at: "2026-09-21T09:05:00Z", posts: 3 },
		{ thread: "standup", first_seq: 61, at: "2026-09-22T00:01:00Z", posts: 1 },
	],
};

describe("sprint page", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_a_sprint", async () => {
		const { container, socket } = await renderApp("/sprints/S2");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", { team: TEAM });
		await waitFor(() =>
			expect(
				s.calls("query").find((f) => f.params.name === "sprint.get")?.params,
			).toEqual({ name: "sprint.get", params: { sprint_id: "S2" } }),
		);
		await answerQuery(s, "sprint.get", SPRINT);

		// The header: the sprint, who started it and who planned it.
		expect(
			await screen.findByRole("heading", { level: 1, name: "Sprint 2" }),
		).toBeTruthy();
		expect(
			screen.getByText("Started Monday 21 September by you. Planned by Sol."),
		).toBeTruthy();
		expect(
			screen.getByRole("button", { name: en.sprintEndEarly }),
		).toBeTruthy();

		// The tasks, each a link with its status in words.
		expect(
			screen.getByText(
				"1 of 3 done. The sprint ends by itself when the last one is accepted or cancelled.",
			),
		).toBeTruthy();
		const tasks = screen.getByRole("list", { name: en.sprintTasks });
		const row = (title: string) =>
			within(tasks)
				.getByRole("link", { name: title })
				.closest("li") as HTMLElement;
		expect(
			within(tasks)
				.getByRole("link", { name: "New checkout page" })
				.getAttribute("href"),
		).toBe("/tasks/FRK-14");
		expect(within(row("New checkout page")).getByText("FRK-14")).toBeTruthy();
		expect(
			within(row("New checkout page")).getByText(en.statusReview),
		).toBeTruthy();
		expect(
			within(row("Show sold-out items")).getByText(en.statusReworked),
		).toBeTruthy();
		expect(within(row("New photos")).getByText(en.statusDone)).toBeTruthy();

		// The meetings, named in words.
		const meetings = screen.getByRole("list", { name: en.sprintMeetings });
		expect(within(meetings).getByText(en.meetingPlanning)).toBeTruthy();
		expect(within(meetings).getByText(en.meetingStandup)).toBeTruthy();
		expect(
			within(meetings).getByText("3 posts on Monday 21 September"),
		).toBeTruthy();
		expect(
			within(meetings).getByText("1 post on Tuesday 22 September"),
		).toBeTruthy();

		// The spending, and the way to the costs by agent.
		expect(
			screen.getByText("$11.84 so far, of the $20.00 you set for this sprint."),
		).toBeTruthy();
		expect(
			screen
				.getByRole("link", { name: en.sprintSeeCosts })
				.getAttribute("href"),
		).toBe("/costs");
		await expectNoAxeViolations(container);

		// Ending it early asks first.
		fireEvent.click(screen.getByRole("button", { name: en.sprintEndEarly }));
		expect(
			within(
				screen.getByRole("dialog", { name: "End sprint 2 early?" }),
			).getByText(
				"2 of its tasks are not finished. They go back on the board.",
			),
		).toBeTruthy();
	});
});
