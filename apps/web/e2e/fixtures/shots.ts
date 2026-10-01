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

/**
 * The page at a desktop's size, once the shell has traded the phone's bar back for its rail, which
 * it does a render after the resize: measured before, the rail's 200 px column is not there yet.
 */
export async function wide(page: Page, width = 1280): Promise<void> {
	await page.setViewportSize({ width, height: 800 });
	await expect
		.poll(() =>
			page
				.getByRole("navigation", { name: "Main" })
				.evaluate((rail) => rail.getBoundingClientRect().width)
				.catch(() => 0),
		)
		.toBeGreaterThan(0);
}

/** The page at a phone's size, which must not scroll sideways, and at a desktop's, for the landing review. */
export async function screenshots(
	page: Page,
	name: string,
	width = 1280,
): Promise<void> {
	await page.evaluate(() => window.scrollTo(0, 0));
	await narrow(page);
	await page.screenshot({ path: `${shots}${name}-360.png`, fullPage: true });
	await wide(page, width);
	await page.screenshot({
		path: `${shots}${name}-${width}.png`,
		fullPage: true,
	});
}

/**
 * Each link `selector` finds that is shorter than WCAG 2.2's 24 px target, or whose words break
 * onto a second line, by its text and height; none, when every one is a whole, big enough target.
 */
export function smallLinks(page: Page, selector: string): Promise<string[]> {
	return page.evaluate(
		(selector) =>
			Array.from(document.querySelectorAll(selector)).flatMap((link) => {
				const range = document.createRange();
				range.selectNodeContents(link);
				const lines = new Set(
					Array.from(range.getClientRects()).map((r) => Math.round(r.top)),
				).size;
				const height = link.getBoundingClientRect().height;
				return height < 24 || lines > 1
					? [`${link.textContent}: ${height} px, ${lines} lines`]
					: [];
			}),
		selector,
	);
}
