import { expect, test } from "@playwright/test";
import { startServe } from "./fixtures/serve.ts";

test("on a phone a page's sticky bar rests above the places bar, never under it", async ({
	page,
}) => {
	const serve = await startServe({ transcripts: [] });
	try {
		await page.setViewportSize({ width: 390, height: 844 });
		await page.goto(serve.url);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		const places = page.getByRole("navigation", { name: "Places" });
		await expect(places).toBeVisible();

		// A page sticks its own bar above `--farik-shell-bar-height`: here a bar of 100 px at the
		// end of a page 3000 px long, scrolled to its middle, as the marketing plan's decision bar is.
		await page.evaluate(() => {
			const long = document.createElement("div");
			long.style.height = "3000px";
			const spacer = document.createElement("div");
			spacer.style.height = "2900px";
			const bar = document.createElement("div");
			bar.id = "probe-bar";
			bar.style.cssText =
				"position:sticky;bottom:var(--farik-shell-bar-height, 0);height:100px";
			long.append(spacer, bar);
			document.querySelector("main")?.append(long);
			window.scrollTo(0, 1200);
		});
		await expect
			.poll(() => page.evaluate(() => window.scrollY))
			.toBeGreaterThan(1000);
		const bar = await page.locator("#probe-bar").boundingBox();
		const tabs = await places.boundingBox();
		if (!bar || !tabs) throw new Error("a bar has no box");
		// The places bar is as tall as the shell says, and the page's bar ends where it begins.
		expect(tabs.height).toBeGreaterThan(40);
		expect(bar.y + bar.height).toBeLessThanOrEqual(tabs.y);
		expect(bar.y + bar.height).toBeGreaterThan(tabs.y - 1);
	} finally {
		await serve.stop();
	}
});
