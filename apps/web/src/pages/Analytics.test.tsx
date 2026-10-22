import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { isTeamAddress } from "../app/teams.ts";
import { en } from "../strings/en.ts";
import { ANALYTICS } from "../test/analytics.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerCalls } from "../test/finding.ts";
import { renderApp } from "../test/render-app.tsx";
import { onlyInTips } from "../test/tips.ts";

describe("analytics", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
	});

	it("shows_usage_workflow_and_blockers", async () => {
		const { hub, container } = await renderApp("/analytics");
		const h = hub as FakeSocket;
		answerCalls(h, {
			"analytics.get": (params: Record<string, unknown>) =>
				params.period === "month"
					? { ...ANALYTICS, period: "month", claude_ai_conversations: 1 }
					: ANALYTICS,
		});
		expect(isTeamAddress("/analytics")).toBe(false);
		await screen.findByRole("heading", { name: en.analyticsTitle, level: 1 });
		const asked = () => h.calls("analytics.get").map((f) => f.params);
		await waitFor(() => expect(asked()).toEqual([{ period: "week" }]));

		// The usage per team: both amounts beside each bar, the watched folders, and the label.
		const usage = await screen.findByRole("region", {
			name: en.analyticsUsage,
		});
		const row = (name: string) =>
			Array.from(usage.querySelectorAll("li")).find((li) =>
				li.textContent?.startsWith(name),
			) as HTMLElement;
		expect(row("Tidepool").textContent).toContain("$41.20 + $18.60");
		expect(row("Bookkeeping").textContent).toContain("$3.80 + $0");
		expect(row("Watched").textContent).toContain("$1.30");
		expect(within(usage).getByText(en.analyticsApiNote)).toBeTruthy();
		// What a subscription's amounts are is said behind the info button, and only there.
		for (const line of en.analyticsApiTip.split("\n")) onlyInTips(line);
		expect(
			within(usage).getByText(
				"claude.ai: 14 conversations this week, no usage figures in the export.",
			),
		).toBeTruthy();
		// The table alternative holds the same numbers.
		const more = within(usage).getByText(en.analyticsTable).closest("details");
		if (!more) throw new Error("no table alternative");
		more.open = true;
		expect(
			within(within(more).getByRole("table", { name: en.analyticsUsageTable }))
				.getByText("Price Watch")
				.closest("tr")?.textContent,
		).toBe("Price Watch$6.10$2.40");

		// The workflow: the four numbers, the sessions each day, and the tools used most.
		const flow = screen.getByRole("region", { name: en.analyticsWorkflow });
		const said = (text: string) => flow.textContent?.includes(text);
		for (const text of [
			"23 sessions",
			"17 h active",
			"31 · 12 commits: you · team",
			"6 pull requests",
			"Editing files · 412",
			"Running commands · 280",
			"Zoho Books · 96",
		])
			expect(said(text), text).toBe(true);
		expect(
			within(flow).getByRole("img", {
				name: "Sunday: Sessions 2",
			}),
		).toBeTruthy();

		// The blockers, one row to a kind, with its count and its examples.
		const blockers = screen.getByRole("region", { name: en.analyticsBlockers });
		for (const text of [
			"Waiting on you · 1Tidepool, 2 days",
			"Failing or silent schedules · 2Hourly price check, Weekly database backup",
			"Failed deploys · 1tidepool-web",
			"Blocked over 3 days · 1Tidepool: “Add Apple sign-in”",
		])
			expect(blockers.textContent, text).toContain(text);
		expect(blockers.textContent).not.toContain("Permission denials");

		// This month asks again for the month.
		fireEvent.click(screen.getByRole("radio", { name: en.analyticsMonth }));
		await waitFor(() =>
			expect(asked()).toEqual([{ period: "week" }, { period: "month" }]),
		);
		await screen.findByText(
			"claude.ai: 1 conversation this month, no usage figures in the export.",
		);
		await expectNoAxeViolations(container);
	});
});
