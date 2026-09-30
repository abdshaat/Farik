import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { farik, startServe } from "./fixtures/serve.ts";

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

test("the team's setup keeps the five, sets the rules, and starts them", async ({
	page,
}) => {
	const serve = await startServe({
		transcripts: [],
		project: true,
		setupPending: true,
	});
	const marker = join(serve.project, ".farik/local/setup-pending");
	try {
		await page.goto(serve.url);
		await expect(page).toHaveURL(/\/setup\/scan$/);
		await expect(page.getByText("What it is")).toBeVisible();
		await page.getByRole("button", { name: "That's right" }).click();

		await expect(page).toHaveURL(/\/setup\/team$/);
		await screenshots(page, "setup-team");
		await page
			.getByRole("button", { name: "Continue with these five" })
			.click();

		await expect(page).toHaveURL(/\/setup\/permissions$/);
		await page.getByLabel(/^Yes, the Developer and Architect may/).check();
		await page.getByLabel(/^No, keep everything on this computer/).check();
		await screenshots(page, "setup-permissions");
		await page.getByRole("button", { name: "Continue", exact: true }).click();

		await expect(page).toHaveURL(/\/setup\/spending$/);
		await page.getByLabel(/^Stop the team after a set amount each day/).check();
		await page.getByLabel("Limit per day, in US dollars").fill("10");
		await page.getByRole("button", { name: "Continue", exact: true }).click();

		await expect(page).toHaveURL(/\/setup\/finish$/);
		await page.getByLabel(/^Farik adds it for me/).check();
		await page.getByRole("button", { name: "Start the team" }).click();
		await expect(page).toHaveURL(/\/events$/);

		const yaml = readFileSync(join(serve.project, ".farik/team.yaml"), "utf8");
		for (const id of ["mira", "sol", "ada", "theo", "kai"])
			expect(yaml).toMatch(new RegExp(`id: ${id}\\b`));
		expect(yaml.match(/^ {2}id: /gm)).toHaveLength(5);
		expect(yaml).toMatch(/daily_usd: 10\b/);
		expect(yaml).toMatch(/run_commands: true/);

		const kinds = farik(serve.project, ["--json", "log"])
			.trim()
			.split("\n")
			.map((line) => JSON.parse(line).kind);
		const updated = kinds.lastIndexOf("team.updated");
		expect(updated).toBeGreaterThan(-1);
		expect(kinds.indexOf("team.resumed", updated)).toBeGreaterThan(updated);
		expect(existsSync(marker)).toBe(false);
		await page.goto(`http://127.0.0.1:${serve.port}/settings`);
		await expect(
			page.getByText("Nothing new starts until you resume."),
		).toHaveCount(0);

		// The pages the team lives on, on a phone and on a desktop.
		await page.goto(`http://127.0.0.1:${serve.port}/team`);
		await expect(page.getByText("Mira").first()).toBeVisible();
		await screenshots(page, "team");
		await page.goto(`http://127.0.0.1:${serve.port}/team/theo`);
		await expect(page.getByLabel("Name", { exact: true })).toBeVisible();
		await screenshots(page, "team-agent");
	} finally {
		await serve.stop();
	}
});
