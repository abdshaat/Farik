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
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerCalls, Refused } from "../test/finding.ts";
import { DIGEST, SESSION_ID } from "../test/reminders.ts";
import { renderApp } from "../test/render-app.tsx";

describe("needs you today", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
	});

	it("shows_what_needs_you_and_acts_on_it", async () => {
		const copied: string[] = [];
		vi.stubGlobal("navigator", {
			...navigator,
			clipboard: {
				writeText: async (text: string) => {
					copied.push(text);
				},
			},
		});
		const asked: string[] = [];
		vi.stubGlobal(
			"Notification",
			Object.assign(function Notification() {}, {
				permission: "default",
				requestPermission: async () => {
					asked.push("asked");
					return "granted";
				},
			}),
		);
		const { hub } = await renderApp("/needs-you");
		const h = hub as FakeSocket;
		const table: Record<string, unknown> = {
			"reminders.get": DIGEST,
			"reminders.give": { task_id: "CTV-7", team: "tidepool" },
			"reminders.snooze": {},
			"reminders.dismiss": {},
		};
		answerCalls(h, table);
		expect(isTeamAddress("/needs-you")).toBe(false);

		// The count, the groups with their headings, and each item's words.
		await screen.findByRole("heading", { name: en.needsYouTitle, level: 1 });
		expect(
			await screen.findByText("9 things · from your 09:00 reminder"),
		).toBeTruthy();
		for (const heading of [
			"Sessions you may have forgotten",
			"Ending soon",
			"Schedules and deploys",
			"Your teams",
			"Services",
		])
			expect(screen.getByRole("heading", { name: heading })).toBeTruthy();
		const item = (title: string) =>
			within(
				screen.getByText(title, { exact: false }).closest("li") as HTMLElement,
			);
		const csv = item("Add CSV export · recipe-cards");
		expect(csv.getByText("4 days ago · 3 to-dos still open")).toBeTruthy();
		expect(
			screen.getByText("Tidepool: Mira has waited 2 days for your answer"),
		).toBeTruthy();
		expect(
			screen.getByText("Zoho Books needs you to sign in again in Claude"),
		).toBeTruthy();
		// Today's runs are collapsed.
		const runs = screen.getByText("Today’s runs (2)").closest("details");
		expect(runs?.open).toBe(false);
		// What a cron and a disabled schedule are is said behind an info button.
		expect(
			within(
				screen
					.getByText("GitHub turned off Weekly database backup", {
						exact: false,
					})
					.closest("li") as HTMLElement,
			).getByRole("button", { name: /More about this/i }),
		).toBeTruthy();

		// Copy resume command copies what the session is resumed with.
		fireEvent.click(csv.getByRole("button", { name: en.needsYouCopyResume }));
		await waitFor(() =>
			expect(copied).toEqual([
				`cd ~/code/recipe-cards && claude --resume ${SESSION_ID}`,
			]),
		);
		// Open on GitHub is a link to the repository's page.
		expect(
			item("Hourly price check failed")
				.getByRole("link", { name: en.needsYouOpenGithub })
				.getAttribute("href"),
		).toBe("https://github.com/octo/price-watch/actions");

		// Give to the team opens "Give to Tidepool" with the request, and Send sends the key.
		fireEvent.click(csv.getByRole("button", { name: en.needsYouGive }));
		const dialog = await screen.findByRole("dialog", {
			name: "Give to Tidepool",
		});
		expect(
			within(dialog).getByText(/Finish “Add CSV export” in recipe-cards\./),
		).toBeTruthy();
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.needsYouSend }),
		);
		const given = await waitFor(() => {
			const frame = h.calls("reminders.give")[0];
			if (!frame) throw new Error("no reminders.give was sent");
			return frame;
		});
		expect(given.params).toEqual({ key: `forgotten:${SESSION_ID}` });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

		// Snooze offers a day and a week.
		fireEvent.click(csv.getByRole("button", { name: en.needsYouSnooze }));
		expect(
			screen.getByRole("button", { name: en.needsYouForADay }),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: en.needsYouForAWeek }));
		const snoozed = await waitFor(() => {
			const frame = h.calls("reminders.snooze")[0];
			if (!frame) throw new Error("no reminders.snooze was sent");
			return frame;
		});
		expect(snoozed.params).toEqual({
			key: `forgotten:${SESSION_ID}`,
			for: "week",
		});

		// Dismiss sends the key.
		fireEvent.click(
			item("Hourly price check failed").getByRole("button", {
				name: en.needsYouDismiss,
			}),
		);
		const dismissed = await waitFor(() => {
			const frame = h.calls("reminders.dismiss")[0];
			if (!frame) throw new Error("no reminders.dismiss was sent");
			return frame;
		});
		expect(dismissed.params).toEqual({
			key: "schedules:failed:octo/price-watch:price.yml",
		});

		// A refusal is said in plain words.
		table["reminders.give"] = new Refused(
			"no_team_for_this",
			"make a team for recipe-cards first",
		);
		fireEvent.click(
			item("Fix the login redirect").getByRole("button", {
				name: en.needsYouGive,
			}),
		);
		fireEvent.click(
			within(await screen.findByRole("dialog")).getByRole("button", {
				name: en.needsYouSend,
			}),
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseNoTeamForThis,
		);

		// Allow notices asks the browser.
		fireEvent.click(screen.getByRole("button", { name: en.needsYouAllow }));
		await waitFor(() => expect(asked).toEqual(["asked"]));
	});
});
