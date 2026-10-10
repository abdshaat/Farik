import { expect, type Page, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

/** Opens the task's acceptance gate from Today's Waiting row, once the review is in. */
async function openTheGate(page: Page) {
	await page.getByRole("link", { name: "Today" }).first().click();
	await expect(page.getByText(/^Accept /)).toBeVisible({ timeout: 30_000 });
	await page.getByRole("link", { name: "Review" }).click();
	await expect(page).toHaveURL(/\/tasks\/CTV-1\/accept$/);
}

test("a high-risk plan is approved, its work sent back once, then accepted", async ({
	page,
}) => {
	// Three sessions play before the gate, and a slow disk stretches each one.
	test.setTimeout(90_000);
	const serve = await startServe({
		team: "pm-architect-developer",
		transcripts: [
			"triage_ctv_1_small_by_pm",
			"refine_writes_high_risk_ctv_1",
			"judge_ctv_1_by_architect",
			"plan_assigns_ctv_1_to_theo",
			"implement_finishes_ctv_1",
			"review_writes_note",
			"implement_after_send_back_ctv_1",
			"review_writes_note",
		],
	});
	try {
		await page.goto(serve.url);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		await expect(page.getByText("Mira").first()).toBeVisible();
		await page
			.getByLabel("What should the team do next?")
			.fill("Add a done.txt at the root, so a run can be checked for it");
		await page.getByRole("button", { name: "Send to the team" }).click();
		await expect(page).toHaveURL(/\/requests\/CTV-1$/);

		// A high-risk plan waits for the user's approval.
		await page.getByRole("link", { name: "Today" }).first().click();
		await expect(page.getByText(/^Approve the plan for /)).toBeVisible();
		await page.getByRole("link", { name: "Review" }).click();
		await page.getByRole("button", { name: "Approve the plan" }).click();

		await openTheGate(page);
		await expect(
			page.getByText(
				"I added done.txt at the root of the project, as the plan asked. Nothing was left out.",
			),
		).toBeVisible();
		await expect(
			page.getByText("Theo, your Developer, wrote this for you"),
		).toBeVisible();
		await expect(
			page.getByText(
				"The work passes. done.txt is there, and the change adds nothing else.",
			),
		).toBeVisible();
		await expect(
			page.getByText("Ada, your Architect, reviewed it"),
		).toBeVisible();
		await page
			.getByRole("button", { name: "See the code changes · 1 file, +0 −0" })
			.click();
		await expect(page.getByText("done.txt").first()).toBeVisible();

		// On a phone both answers fit without scrolling sideways.
		await screenshots(page, "gate");
		await page.setViewportSize({ width: 360, height: 780 });
		for (const name of ["Accept the work", "Send back with a note"]) {
			const button = page.getByRole("button", { name });
			await expect(button).toBeVisible();
			const box = await button.boundingBox();
			expect((box?.x ?? 0) + (box?.width ?? Infinity)).toBeLessThanOrEqual(360);
		}
		// On a desktop the rail stays in place while the page scrolls.
		await page.setViewportSize({ width: 1280, height: 800 });
		const rail = page.getByRole("navigation", { name: "Main" });
		const before = await rail.boundingBox();
		await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
		expect(await page.evaluate(() => window.scrollY)).toBeGreaterThan(0);
		expect((await rail.boundingBox())?.y).toBe(before?.y);

		await page.getByRole("button", { name: "Send back with a note" }).click();
		const dialog = page.getByRole("dialog");
		await dialog.getByLabel("Something else").check();
		await dialog
			.getByLabel("Your note to Theo")
			.fill("Please write the date of the run in done.txt.");
		await screenshots(page, "send-back");
		await dialog.getByRole("button", { name: "Send back to Theo" }).click();
		await expect
			.poll(() =>
				events(serve.project).some(
					(e) => e.kind === "task.transitioned" && e.body.to === "rejected",
				),
			)
			.toBe(true);

		// Theo writes the date, Ada reviews it again, and it comes back to the user.
		await openTheGate(page);
		await expect(
			page.getByText(
				"done.txt now holds the date of the run, as your note asked. Nothing was left out.",
			),
		).toBeVisible();
		await page.getByRole("button", { name: "Accept the work" }).click();
		await expect(page).toHaveURL(/:\d+\/$/);
		await expect
			.poll(() =>
				events(serve.project)
					.filter((e) => e.kind === "human.accepted")
					.map((e) => e.body.subject),
			)
			.toEqual(["contract", "result"]);
	} finally {
		await serve.stop();
	}
});
