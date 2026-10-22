import { expect, test } from "@playwright/test";
import { startServe } from "./fixtures/serve.ts";

test("a new owner finds their projects and makes their teams", async ({
	page,
}) => {
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

		// 2. Both projects are listed with Team chosen; one goes to Watch and back, and Continue keeps it.
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
		await cards.getByText("Team", { exact: true }).click();
		await expect(cards.getByRole("radio", { name: "Team" })).toBeChecked();
		await page.getByRole("button", { name: "Continue" }).click();

		// 3. Nothing proves the one project works with another: only the end of the questions.
		await expect(page).toHaveURL(/\/find\/together$/);
		await expect(
			page.getByRole("heading", { name: "That's all we could prove" }),
		).toBeVisible();
		await page.getByRole("button", { name: "Continue" }).click();

		// 4. Your teams: a card for each project, nothing grouped. Create makes both, paused.
		await expect(page).toHaveURL(/\/find\/teams$/);
		await expect(
			page.getByRole("heading", { name: "Your teams", level: 1 }),
		).toBeVisible();
		await expect(
			page.getByRole("heading", { name: "corner-bakery", level: 2 }),
		).toBeVisible();
		await expect(
			page.getByRole("heading", { name: "recipe-cards", level: 2 }),
		).toBeVisible();
		await page.getByRole("button", { name: "Create 2 teams" }).click();

		// 5. All teams says so; both teams are paused and can be undone until they start.
		await expect(page).toHaveURL(/\/teams$/);
		await expect(
			page.getByText("2 teams created. They're paused until you start them."),
		).toBeVisible();
		for (const name of ["corner-bakery", "recipe-cards"]) {
			const card = page.locator("article").filter({
				has: page.getByRole("heading", { name, level: 2 }),
			});
			await expect(card.getByText("Paused")).toBeVisible();
			await expect(card.getByRole("button", { name: "Undo" })).toBeVisible();
			await expect(card.getByRole("button", { name: "Start" })).toBeVisible();
		}

		// What was found is kept on this computer: the list shows both as teams now.
		await page.goto(`http://127.0.0.1:${serve.port}/find/found`);
		for (const name of ["recipe-cards", "corner-bakery"]) {
			await expect(
				page.getByRole("listitem").filter({ hasText: name }),
			).toContainText("Already a team");
		}
	} finally {
		await serve.stop();
	}
});
