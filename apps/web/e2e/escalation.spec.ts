import { expect, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { narrow, screenshots, wide } from "./fixtures/shots.ts";

test("an escalation is answered from Today in one action", async ({ page }) => {
	// Seven sessions play, and a slow disk stretches each one.
	test.setTimeout(90_000);
	const serve = await startServe({
		team: "pm-architect-developer",
		transcripts: [
			"triage_ctv_1_small_by_pm",
			"refine_writes_four_session_task_for_theo_ctv_1",
			"plan_assigns_ctv_1_to_theo",
			"implement_stops_early",
			"implement_finishes_ctv_1",
			"review_writes_note",
			"accept_ctv_1",
		],
	});
	try {
		await page.goto(serve.url);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		await expect(page.getByText("Theo").first()).toBeVisible();
		await page
			.getByLabel("What should the team do next?")
			.fill("Add a done.txt at the root, so a run can be checked for it");
		await page.getByRole("button", { name: "Send to the team" }).click();
		await expect(page).toHaveURL(/\/requests\/CTV-1$/);

		await page.getByRole("link", { name: "Today" }).first().click();
		const row = page.getByRole("listitem").filter({
			has: page.getByText("Theo needs your help", { exact: true }),
		});
		await expect(row).toBeVisible({ timeout: 60_000 });
		await page.getByRole("link", { name: "Answer" }).click();
		await expect(page).toHaveURL(/\/tasks\/CTV-1\/help$/);

		const room = page.getByRole("listitem").filter({
			has: page.getByRole("button", { name: "Give it more room" }),
		});
		await expect(room.getByText("Suggested", { exact: true })).toBeVisible();
		await expect(
			room.getByText(
				"Theo carries on with CTV-1, with up to 4 more sessions and $5.00 more to spend.",
			),
		).toBeVisible();
		await expect(
			page.getByRole("button", { name: "Cancel the task" }),
		).toBeVisible();
		await expect(page.getByRole("radio")).toHaveCount(0);
		await expect(page.getByRole("combobox")).toHaveCount(0);
		await screenshots(page, "escalation");

		await narrow(page);
		const rights = await page
			.locator("main button, main a.action")
			.evaluateAll((els) => els.map((el) => el.getBoundingClientRect().right));
		for (const right of rights) expect(right).toBeLessThanOrEqual(360);
		await wide(page);

		// Giving room needs no note (ADR 0079): only an answer does.
		await expect(
			page.getByRole("button", { name: "Give it more room" }),
		).toBeEnabled();
		await page.getByRole("button", { name: "Give it more room" }).click();
		await expect(page).toHaveURL(`${new URL(serve.url).origin}/`);
		await expect(
			page.getByText("Theo needs your help", { exact: true }),
		).toHaveCount(0);

		const log = events(serve.project);
		expect(log.filter((e) => e.kind === "task.room_granted")).toMatchObject([
			{ body: { sessions: 4, cost_usd: 5, granted_by: "human" } },
		]);
		expect(log.filter((e) => e.kind === "escalation.resolved")).toMatchObject([
			// The budget counts work sessions only (ADR 0079), so the fourth is the review, and the
			// task goes back to the review it was escalated from.
			{ body: { to: "verifying", resolved_by: "human" } },
		]);

		await expect
			.poll(
				() =>
					events(serve.project).some(
						(e) =>
							e.kind === "task.transitioned" &&
							e.task_id === "CTV-1" &&
							e.body.to === "accepted",
					),
				{ timeout: 60_000 },
			)
			.toBe(true);
		expect(
			events(serve.project).filter((e) => e.kind === "escalation.raised"),
		).toHaveLength(1);
	} finally {
		await serve.stop();
	}
});
