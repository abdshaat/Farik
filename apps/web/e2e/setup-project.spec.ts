import { existsSync, mkdtempSync, realpathSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { catervas, gitProject, startServe } from "./fixtures/serve.ts";

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

test("a first run checks the computer, signs in through Claude Code, and takes the project on", async ({
	page,
}) => {
	// A home with one git project, and Catervas started in an empty folder, with a Claude Code that is not signed in.
	const home = realpathSync(mkdtempSync(join(tmpdir(), "catervas-e2e-home-")));
	const project = join(home, "Projects", "corner-bakery");
	gitProject(project);
	const serve = await startServe({ transcripts: [], project: false, home });
	try {
		await page.goto(serve.url);
		await expect(page).toHaveURL(/\/setup\/computer$/);

		// The fake claude is ready; the fake docker's daemon does not answer.
		await expect(page.getByText("Version 2.1.300 found.")).toBeVisible();
		// "Ready" sits on the pale green pill: the Added tint, in either theme.
		await expect(page.getByText("Ready", { exact: true }).first()).toHaveCSS(
			"background-color",
			"rgb(227, 235, 223)",
		);
		await expect(page.getByText("Not running", { exact: true })).toBeVisible();
		await expect(
			page.getByRole("button", { name: "Continue", exact: true }),
		).toBeDisabled();
		await screenshots(page, "setup-computer");
		await page.getByRole("button", { name: "Continue without Docker" }).click();

		await expect(page).toHaveURL(/\/setup\/account$/);
		await page.evaluate(() => {
			// The page tries to open Claude's sign-in page in a new tab: the journey only records it.
			const opened: string[] = [];
			(window as unknown as { opened: string[] }).opened = opened;
			window.open = (url) => {
				opened.push(String(url));
				return null;
			};
		});
		await screenshots(page, "setup-account");
		await page
			.getByRole("button", { name: "Sign in with Claude (Pro or Max)" })
			.click();
		await expect(
			page.getByText(/Sign in on the page that opened/),
		).toBeVisible();
		expect(
			await page.evaluate(
				() => (window as unknown as { opened: string[] }).opened,
			),
		).toEqual(["https://claude.com/cai/oauth/authorize?fake=1"]);
		await page.getByLabel("Your sign-in code").fill("e2e-code");
		await page.getByRole("button", { name: "Finish signing in" }).click();

		await expect(page).toHaveURL(/\/setup\/project$/);
		// Catervas kept no key: Claude Code holds the sign-in.
		expect(existsSync(join(home, ".config/catervas/credential.json"))).toBe(
			false,
		);
		expect(existsSync(join(home, ".fake-claude-signed-in"))).toBe(true);
		// Back on the account step, Claude Code is signed in and the check works.
		await page.getByRole("button", { name: "Back" }).click();
		await expect(page).toHaveURL(/\/setup\/account$/);
		await expect(page.getByText("It works.")).toBeVisible();
		await page.getByRole("button", { name: "Continue" }).click();
		await expect(page).toHaveURL(/\/setup\/project$/);
		await screenshots(page, "setup-question");
		await page.getByRole("button", { name: "Continue" }).click();
		const folders = page.getByRole("group", { name: "Folders" });
		await folders.getByRole("button", { name: /^Projects/ }).dblclick();
		await folders.getByRole("button", { name: /^corner-bakery/ }).click();
		await screenshots(page, "setup-folders");
		await page.getByRole("button", { name: "Use this folder" }).click();

		// Catervas restarts on the project, the page reconnects by itself, and the team's setup
		// starts from what the scan found.
		await expect(page).toHaveURL(`http://127.0.0.1:${serve.port}/setup/scan`, {
			timeout: 30_000,
		});
		await expect(
			page.getByRole("heading", {
				name: "Here is what Catervas found in your project",
			}),
		).toBeVisible();
		await expect(page.getByText("What it is")).toBeVisible();
		await screenshots(page, "setup-opened");
		// Until the team is set up, a deep link goes back to where setup stands.
		await page.goto(`http://127.0.0.1:${serve.port}/settings`);
		await expect(page).toHaveURL(`http://127.0.0.1:${serve.port}/setup/scan`);
		const kinds = catervas(project, ["--json", "log"])
			.trim()
			.split("\n")
			.map((line) => JSON.parse(line).kind);
		expect(kinds).toContain("team.paused");
	} finally {
		await serve.stop();
	}
});
