import { expect, type Page, test } from "@playwright/test";
import { catervas, startServe } from "./fixtures/serve.ts";

const shots = new URL("./screenshots/", import.meta.url).pathname;

/** Screenshots of the page as it is, at a phone's size and a desktop's, for the landing review. */
async function screenshots(page: Page, name: string) {
	// The Pause control shows once `serve.status` has answered, so the shots hold its data.
	const control = page.getByRole("button", { name: /^(Pause|Resume)/ });
	await page.setViewportSize({ width: 360, height: 780 });
	await expect(control).toBeVisible();
	await page.screenshot({ path: `${shots}${name}-360.png`, fullPage: true });
	await page.setViewportSize({ width: 1280, height: 800 });
	await expect(control).toBeVisible();
	await page.screenshot({ path: `${shots}${name}-1280.png`, fullPage: true });
}

/** How many lines an element's text takes. */
function lines(element: Element): number {
	const range = document.createRange();
	range.selectNodeContents(element);
	return new Set([...range.getClientRects()].map((r) => r.top)).size;
}

test("the start link opens the app and pause works end to end", async ({
	page,
}) => {
	const serve = await startServe({ transcripts: [] });
	try {
		await page.goto(serve.url);
		await expect(page).toHaveURL(`http://127.0.0.1:${serve.port}/`);
		await expect(page.getByText("Connected", { exact: true })).toBeVisible();
		await screenshots(page, "connected");

		const banner = page.getByText("Nothing new starts until you resume.");
		await page.getByRole("button", { name: "Pause the team" }).click();
		await expect(banner).toBeVisible();
		// The rail's Resume keeps to one line.
		const resume = page.getByRole("button", { name: "Resume the team" });
		expect(await resume.evaluate(lines)).toBe(1);
		const kinds = catervas(serve.project, ["--json", "log"])
			.trim()
			.split("\n")
			.map((line) => JSON.parse(line).kind);
		expect(kinds).toContain("team.paused");
		await screenshots(page, "paused");

		await page.getByRole("button", { name: "Resume the team" }).click();
		await expect(banner).toBeHidden();

		await page.reload();
		await expect(page).toHaveURL(`http://127.0.0.1:${serve.port}/`);
		await expect(page.getByText("Connected", { exact: true })).toBeVisible();
	} finally {
		await serve.stop();
	}
});

test("a used link and a lost connection are said plainly", async ({
	page,
	browser,
}) => {
	const serve = await startServe({ transcripts: [] });
	try {
		await page.goto(serve.url);
		await expect(page.getByText("Connected", { exact: true })).toBeVisible();

		// A new context has no cookie, so only the link could let it in, and the link is spent.
		const second = await browser.newContext();
		const again = await second.newPage();
		await again.goto(serve.url);
		await expect(
			again.getByText(/This start link was already used/),
		).toBeVisible();
		// It fits a phone, and its Copy button is outlined, as in the Connect mockup.
		await again.setViewportSize({ width: 360, height: 780 });
		expect(
			await again.evaluate(() => document.documentElement.scrollWidth),
		).toBeLessThanOrEqual(360);
		const copy = again.getByRole("button", { name: "Copy" });
		await expect(copy).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
		await second.close();
	} finally {
		await serve.stop();
	}
	await expect(page.getByText("Catervas stopped answering")).toBeVisible();
});
