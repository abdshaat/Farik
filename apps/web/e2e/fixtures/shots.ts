import { expect, type Page } from "@playwright/test";

const shots = new URL("../screenshots/", import.meta.url).pathname;

/**
 * The page at 360 px does not scroll sideways. The shell trades its rail for the phone's bar a
 * render after the resize, so it is asked until it settles; if it never does, the failure names
 * each element wider than the screen, so the one that widens it can be found.
 */
export async function narrow(page: Page): Promise<void> {
	await page.setViewportSize({ width: 360, height: 780 });
	await expect
		.poll(() =>
			page.evaluate(() => {
				const width = document.documentElement.scrollWidth;
				if (width <= 360) return [];
				const wide = Array.from(document.querySelectorAll("body *"))
					.filter((el) => el.getBoundingClientRect().right > 360)
					.map(
						(el) =>
							`${el.tagName}.${el.className} ${Math.round(el.getBoundingClientRect().right)} ${(el.textContent ?? "").slice(0, 40)}`,
					);
				return [`the page is ${width} px wide`, ...wide];
			}),
		)
		.toEqual([]);
}

/** The page at a phone's size, which must not scroll sideways, and at a desktop's, for the landing review. */
export async function screenshots(page: Page, name: string): Promise<void> {
	await page.evaluate(() => window.scrollTo(0, 0));
	await narrow(page);
	await page.screenshot({ path: `${shots}${name}-360.png`, fullPage: true });
	await page.setViewportSize({ width: 1280, height: 800 });
	await page.screenshot({ path: `${shots}${name}-1280.png`, fullPage: true });
}
