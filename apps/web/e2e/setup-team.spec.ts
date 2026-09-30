import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { farik, startServe } from "./fixtures/serve.ts";
import { narrow } from "./fixtures/shots.ts";

const shots = new URL("./screenshots/", import.meta.url).pathname;

/** Every picture on the page has loaded, so a screenshot is not taken half-drawn. */
async function pictured(page: Page) {
	await page.waitForFunction(() =>
		Array.from(document.images).every(
			(img) => img.complete && img.naturalWidth > 0,
		),
	);
}

/** The screen as it is, at a desktop's size and a phone's, for the landing review. */
async function screenshots(page: Page, name: string) {
	await pictured(page);
	await narrow(page);
	await pictured(page);
	await page.screenshot({ path: `${shots}${name}-360.png`, fullPage: true });
	await page.setViewportSize({ width: 1280, height: 800 });
	await pictured(page);
	await page.screenshot({ path: `${shots}${name}-1280.png`, fullPage: true });
}

test("the team's setup keeps the six, sets the rules, and starts them", async ({
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
		// Six reads the general words until step 11 task 5 gives six its own.
		await page.getByRole("button", { name: "Continue with this team" }).click();

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
		await expect(page).toHaveURL(/:\d+\/$/);

		const yaml = readFileSync(join(serve.project, ".farik/team.yaml"), "utf8");
		for (const id of ["mira", "sol", "ada", "theo", "iris", "kai"])
			expect(yaml).toMatch(new RegExp(`id: ${id}\\b`));
		expect(yaml.match(/^ {2}id: /gm)).toHaveLength(6);
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

		// After setup, the same answers are changed from Settings, each effect shown first.
		const finish = page.getByRole("region", {
			name: "How finished work is added",
		});
		await finish.getByLabel(/^I will add each one myself/).check();
		await expect(
			finish.getByText("Finished work waits for you to merge it."),
		).toBeVisible();
		await screenshots(page, "settings");
		await finish.getByRole("button", { name: "Save changes" }).click();
		await expect
			.poll(() => readFileSync(join(serve.project, ".farik/team.yaml"), "utf8"))
			.toMatch(/integration: manual/);
		await expect(
			page
				.getByRole("region", { name: "How finished work is added" })
				.getByLabel(/^I will add each one myself/),
		).toBeChecked();

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
