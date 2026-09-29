import { existsSync, mkdtempSync, realpathSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { farik, gitProject, startServe } from "./fixtures/serve.ts";

const shots = new URL("./screenshots/", import.meta.url).pathname;

/** The screen as it is, at a desktop's size and a phone's, for the landing review. */
async function screenshots(page: Page, name: string) {
	await page.setViewportSize({ width: 360, height: 780 });
	expect(
		await page.evaluate(() => document.documentElement.scrollWidth),
	).toBeLessThanOrEqual(360);
	await page.screenshot({ path: `${shots}${name}-360.png`, fullPage: true });
	await page.setViewportSize({ width: 1280, height: 800 });
	await page.screenshot({ path: `${shots}${name}-1280.png`, fullPage: true });
}

test("a first run checks the computer, keeps the key, and takes the project on", async ({
	page,
}) => {
	// A home with one git project, and Farik started in an empty folder with no key anywhere.
	const home = realpathSync(mkdtempSync(join(tmpdir(), "farik-e2e-home-")));
	const project = join(home, "Projects", "corner-bakery");
	gitProject(project);
	const serve = await startServe({ transcripts: [], project: false, home });
	try {
		await page.goto(serve.url);
		await expect(page).toHaveURL(/\/setup\/computer$/);

		// The fake claude is ready; the fake docker's daemon does not answer.
		await expect(page.getByText("Version 2.1.300 found.")).toBeVisible();
		await expect(page.getByText("Not running", { exact: true })).toBeVisible();
		await expect(
			page.getByRole("button", { name: "Continue", exact: true }),
		).toBeDisabled();
		await screenshots(page, "setup-computer");
		await page.getByRole("button", { name: "Continue without Docker" }).click();

		await expect(page).toHaveURL(/\/setup\/account$/);
		await page.getByLabel("Your subscription key").fill("sk-ant-oat01-test");
		await screenshots(page, "setup-account");
		await page.getByRole("button", { name: "Save and continue" }).click();

		await expect(page).toHaveURL(/\/setup\/project$/);
		// No keychain here, so the key went to a private file in Farik's state folder.
		await expect(page.getByText(/saved in a private file/)).toBeVisible();
		expect(existsSync(join(home, ".config/farik/credential.json"))).toBe(true);
		await screenshots(page, "setup-question");
		await page.getByRole("button", { name: "Continue" }).click();
		const folders = page.getByRole("group", { name: "Folders" });
		await folders.getByRole("button", { name: /^Projects/ }).dblclick();
		await folders.getByRole("button", { name: /^corner-bakery/ }).click();
		await screenshots(page, "setup-folders");
		await page.getByRole("button", { name: "Use this folder" }).click();

		// Farik restarts on the project, the page reconnects by itself, and the team's setup
		// starts from what the scan found.
		await expect(page).toHaveURL(`http://127.0.0.1:${serve.port}/setup/scan`, {
			timeout: 30_000,
		});
		await expect(
			page.getByRole("heading", {
				name: "Here is what Farik found in your project",
			}),
		).toBeVisible();
		await expect(page.getByText("What it is")).toBeVisible();
		await screenshots(page, "setup-opened");
		await page.goto(`http://127.0.0.1:${serve.port}/settings`);
		await expect(page.getByText("Connected", { exact: true })).toBeVisible();
		await expect(
			page.getByText("Nothing new starts until you resume."),
		).toBeVisible();
		await expect(page.getByText(project, { exact: true })).toBeVisible();
		const kinds = farik(project, ["--json", "log"])
			.trim()
			.split("\n")
			.map((line) => JSON.parse(line).kind);
		expect(kinds).toContain("team.paused");
	} finally {
		await serve.stop();
	}
});
