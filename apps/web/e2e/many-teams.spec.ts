import { basename } from "node:path";
import { expect, test } from "@playwright/test";
import { startServe } from "./fixtures/serve.ts";

test("the owner moves between two teams and sees both on All teams", async ({
	page,
}) => {
	const serve = await startServe({ transcripts: [], teams: 2 });
	try {
		const [first, second] = serve.projects.map((project) => basename(project));
		if (!first || !second) throw new Error("two projects were made");

		// 1. Two teams and none remembered: the link lands on All teams, with both cards.
		await page.goto(serve.url);
		await expect(page).toHaveURL(/\/teams$/);
		await expect(
			page.getByRole("heading", { name: "All teams", level: 1 }),
		).toBeVisible();
		await expect(
			page.getByRole("heading", { name: first, level: 2 }),
		).toBeVisible();
		await expect(
			page.getByRole("heading", { name: second, level: 2 }),
		).toBeVisible();

		// 2. The first team's Today, then the second's from the switcher in the rail.
		await page.getByRole("link", { name: first }).first().click();
		await expect(page).toHaveURL(/\/t\/[^/]+\/$/);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		await page.getByRole("button", { name: first }).click();
		await page
			.getByRole("menu", { name: "Teams" })
			.getByRole("menuitem", { name: second })
			.click();
		await expect(page.getByRole("button", { name: second })).toBeVisible();
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		const secondUrl = page.url();

		// 3. An old address opens the same page of the team viewed last.
		await page.goto(new URL("/board", page.url()).toString());
		await expect(page).toHaveURL(`${secondUrl}board`);
		await expect(page.getByRole("heading", { name: "Board" })).toBeVisible();
	} finally {
		await serve.stop();
	}
});
