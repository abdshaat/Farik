import { expect, test } from "@playwright/test";
import { startServe } from "./fixtures/serve.ts";

test("a new owner finds their projects", async ({ page }) => {
	// A home with a little Claude Code history of two projects, and Catervas started in an empty folder.
	const serve = await startServe({
		transcripts: [],
		project: false,
		claudeHistory: true,
	});
	try {
		await page.goto(serve.url);
		await expect(page).toHaveURL(/\/setup\/computer$/);

		// 1. The computer check offers to find the projects; the scan ends with the way on.
		await expect(page.getByText("Find your projects?")).toBeVisible();
		await page.getByRole("button", { name: "Look for my projects" }).click();
		await expect(page).toHaveURL(/\/find\/scanning$/);
		await page.getByRole("button", { name: "See what you work on" }).click();

		// 2. Both projects are listed with Team chosen; one goes to Watch, and Continue keeps it.
		await expect(page).toHaveURL(/\/find\/found$/);
		await expect(
			page.getByRole("heading", { name: "Here's what you work on" }),
		).toBeVisible();
		const bakery = page
			.getByRole("listitem")
			.filter({ hasText: "corner-bakery" });
		const cards = page
			.getByRole("listitem")
			.filter({ hasText: "recipe-cards" });
		await expect(bakery.getByRole("radio", { name: "Team" })).toBeChecked();
		await expect(cards.getByRole("radio", { name: "Team" })).toBeChecked();
		await cards.getByText("Watch", { exact: true }).click();
		await expect(cards.getByRole("radio", { name: "Watch" })).toBeChecked();
		await page.getByRole("button", { name: "Continue" }).click();
		await expect(page).toHaveURL(/\/teams$/);

		// The answer was kept on this computer: the list shows it again.
		await page.goto(`http://127.0.0.1:${serve.port}/find/found`);
		await expect(
			page
				.getByRole("listitem")
				.filter({ hasText: "recipe-cards" })
				.getByRole("radio", { name: "Watch" }),
		).toBeChecked();
		await expect(
			page
				.getByRole("listitem")
				.filter({ hasText: "corner-bakery" })
				.getByRole("radio", { name: "Team" }),
		).toBeChecked();
	} finally {
		await serve.stop();
	}
});
