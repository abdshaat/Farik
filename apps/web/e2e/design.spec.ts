import { expect, test } from "@playwright/test";
import { startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

test("a Designer's plan is approved by Catervas at once and shown on its task page", async ({
	page,
}) => {
	const serve = await startServe({
		team: "pm-architect-developer-designer",
		transcripts: [
			"triage_ctv_1_small_by_pm",
			"refine_writes_task_for_iris_ctv_1",
			"plan_assigns_ctv_1_to_iris",
			"explore_plans_ctv_1",
		],
	});
	try {
		await page.goto(serve.url);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		await expect(page.getByText("Mira").first()).toBeVisible();
		await page
			.getByLabel("What should the team do next?")
			.fill("Give the sign-in page room on a phone");
		await page.getByRole("button", { name: "Send to the team" }).click();
		await expect(page).toHaveURL(/\/requests\/CTV-1$/);

		// Iris explores and plans; Catervas approves the plan at once, no one waits on it, and her
		// building session waits, unplayed.
		await page.goto(`http://127.0.0.1:${serve.port}/tasks/CTV-1`);
		await expect(page.getByText("Being built").first()).toBeVisible({
			timeout: 20_000,
		});
		await expect(page.getByText(/^Waiting for .* to approve/)).toHaveCount(0);
		await page.getByRole("tab", { name: "The plan" }).click();
		const panel = page.getByRole("tabpanel");
		await expect(
			panel.getByText(
				"Iris looked at your app and wrote this, then changes the page as it says. The finished change is reviewed, as for any task.",
			),
		).toBeVisible();
		await expect(panel.getByText(/^Catervas approved the plan /)).toBeVisible();
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
