import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { catervas, startServe } from "./fixtures/serve.ts";
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

test("the team's setup starts the suggested team this computer can run, sets the rules, and starts them", async ({
	page,
}) => {
	const serve = await startServe({
		transcripts: [],
		project: true,
		setupPending: true,
	});
	const marker = join(serve.project, ".catervas/local/setup-pending");
	try {
		await page.goto(serve.url);
		await expect(page).toHaveURL(/\/setup\/scan$/);
		await expect(page.getByText("What it is")).toBeVisible();
		await page.getByRole("button", { name: "That's right" }).click();

		await expect(page).toHaveURL(/\/setup\/team$/);
		await screenshots(page, "setup-team");
		// Sandboxing is off here, so the Designer, who needs Docker's sandbox, is held off.
		const iris = page.getByRole("button", { name: "Change Iris" });
		await expect(iris).toContainText("Needs Docker");
		await iris.click();
		await expect(
			page.getByRole("dialog").getByText("Needs Docker’s sandbox"),
		).toBeVisible();
		await page.getByRole("button", { name: "Done" }).click();
		await page.getByRole("button", { name: "Continue with this team" }).click();

		await expect(page).toHaveURL(/\/setup\/permissions$/);
		await expect(
			page.getByLabel(/^Yes, the Developer and Architect may/),
		).toBeChecked();
		await expect(
			page.getByLabel(/^No, keep work in progress on this computer/),
		).toBeChecked();
		await screenshots(page, "setup-permissions");
		await page.getByRole("button", { name: "Continue", exact: true }).click();

		await expect(page).toHaveURL(/\/setup\/spending$/);
		await page.getByLabel(/^Stop the team after a set amount each day/).check();
		await page.getByLabel("Limit per day, in US dollars").fill("10");
		await page.getByRole("button", { name: "Continue", exact: true }).click();

		await expect(page).toHaveURL(/\/setup\/finish$/);
		// No choice is left, and this project has no origin: finished work waits for a merge by hand.
		await expect(
			page.getByText(
				"Your project isn\u2019t online, so finished work stays on this computer for you to merge yourself.",
			),
		).toBeVisible();
		await page.getByRole("button", { name: "Start the team" }).click();
		// An existing project is offered a Catervafication; "Later" goes on to Today.
		await expect(page).toHaveURL(/\/setup\/ready$/);
		await page.getByRole("button", { name: "Later" }).click();
		await expect(page).toHaveURL(/:\d+\/$/);

		const yaml = readFileSync(
			join(serve.project, ".catervas/team.yaml"),
			"utf8",
		);
		for (const id of ["mira", "ada", "theo", "kai"])
			expect(yaml).toMatch(new RegExp(`id: ${id}\\b`));
		expect(yaml).not.toMatch(/id: iris\b/);
		expect(yaml.match(/^ {2}id: /gm)).toHaveLength(4);
		expect(yaml).toMatch(/daily_usd: 10\b/);
		expect(yaml).toMatch(/run_commands: true/);

		const kinds = catervas(serve.project, ["--json", "log"])
			.trim()
			.split("\n")
			.map((line) => JSON.parse(line).kind);
		const updated = kinds.lastIndexOf("team.updated");
		expect(updated).toBeGreaterThan(-1);
		expect(kinds.indexOf("team.resumed", updated)).toBeGreaterThan(updated);
		expect(existsSync(marker)).toBe(false);
		await page.goto(`http://127.0.0.1:${serve.port}/settings/work`);
		await expect(
			page.getByText("Nothing new starts until you resume."),
		).toHaveCount(0);

		// After setup, Settings says the same, each effect shown first.
		const finish = page.getByRole("region", {
			name: "How finished work is added",
		});
		await expect(
			finish.getByText(
				"Your project isn\u2019t online, so finished work stays on this computer for you to merge yourself.",
			),
		).toBeVisible();
		await expect(finish.getByRole("radio")).toHaveCount(0);
		await screenshots(page, "settings");
		expect(yaml).toMatch(/integration: pull_request/);

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
