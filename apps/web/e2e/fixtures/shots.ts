import { expect, type Page } from "@playwright/test";

const shots = new URL("../screenshots/", import.meta.url).pathname;

/** The page at a phone's size, which must not scroll sideways, and at a desktop's, for the landing review. */
export async function screenshots(page: Page, name: string): Promise<void> {
	await page.evaluate(() => window.scrollTo(0, 0));
	await page.setViewportSize({ width: 360, height: 780 });
	// The shell trades its rail for the phone's bar a render after the resize.
	await expect
		.poll(() => page.evaluate(() => document.documentElement.scrollWidth))
		.toBeLessThanOrEqual(360);
	await page.screenshot({ path: `${shots}${name}-360.png`, fullPage: true });
	await page.setViewportSize({ width: 1280, height: 800 });
	await page.screenshot({ path: `${shots}${name}-1280.png`, fullPage: true });
}
