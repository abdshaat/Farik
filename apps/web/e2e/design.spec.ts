import { expect, test } from "@playwright/test";
import { startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

test("a Designer's plan waits for the Product Manager on its task page", async ({
	page,
}) => {
	const serve = await startServe({
		team: "pm-architect-developer-designer",
		transcripts: [
			"triage_frk_1_small_by_pm",
			"refine_writes_task_for_iris_frk_1",
			"judge_frk_1_by_architect",
			"plan_assigns_frk_1_to_iris",
			"explore_plans_frk_1",
		],
	});
	try {
		await page.goto(serve.url);
		await page
			.getByLabel("What should the team do next?")
			.fill("Give the sign-in page room on a phone");
		await page.getByRole("button", { name: "Send to the team" }).click();
		await expect(page).toHaveURL(/\/requests\/FRK-1$/);

		// Iris explores and plans; Mira's decision session then waits, unplayed.
		await page.goto(`http://127.0.0.1:${serve.port}/tasks/FRK-1`);
		await expect(
			page.getByText("Waiting for Mira to approve the plan").first(),
		).toBeVisible({ timeout: 20_000 });
		await page.getByRole("tab", { name: "The plan" }).click();
		const panel = page.getByRole("tabpanel");
		await expect(
			panel.getByText(
				"Iris looked at your app and wrote this. Iris changes nothing until Mira approves it.",
			),
		).toBeVisible();
		await expect(
			panel.getByText(/^Iris wrote this plan (today|on .+) at \d\d:\d\d$/),
		).toBeVisible();
		await expect(
			panel.getByText(/^The sign-in page is crowded at phone width/),
		).toBeVisible();
		await screenshots(page, "design-plan");
	} finally {
		await serve.stop();
	}
});
