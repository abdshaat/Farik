import { expect, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { narrow, screenshots } from "./fixtures/shots.ts";

test("a request is sized, its question answered by choice, and it becomes a task to do", async ({
	page,
}) => {
	const serve = await startServe({
		team: "pm-architect-developer",
		transcripts: [
			"triage_frk_1_small_by_pm",
			"ask_with_choices_frk_1",
			"refine_writes_task_for_theo_frk_1",
			"judge_frk_1_by_architect",
		],
	});
	try {
		await page.goto(serve.url);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		await expect(page.getByText("Theo").first()).toBeVisible();
		await screenshots(page, "today");
		await page
			.getByLabel("What should the team do next?")
			.fill("Add a done.txt at the root, so a run can be checked for it");
		await page.getByRole("button", { name: "Send to the team" }).click();

		await expect(page).toHaveURL(/\/requests\/FRK-1$/);
		await expect(
			page.getByText("Mira sized it as a small request"),
		).toBeVisible();
		await screenshots(page, "request");

		await page.getByRole("link", { name: "Today" }).first().click();
		await expect(page.getByText("Mira has a question")).toBeVisible();
		await screenshots(page, "today-waiting");
		// The row's button is a full-size target: centred on its row and at its right end on a
		// desktop, below the words and across the row on a phone.
		const answer = page.getByRole("link", { name: "Answer" });
		const row = page.getByRole("listitem").filter({ has: answer });
		const box = async () => {
			const [a, r, words] = await Promise.all([
				answer.boundingBox(),
				row.boundingBox(),
				row.locator("strong").boundingBox(),
			]);
			if (!a || !r || !words) throw new Error("the Answer row is not laid out");
			return { a, r, words };
		};
		const wide = await box();
		expect(wide.a.height).toBeGreaterThanOrEqual(44);
		expect(wide.r.x + wide.r.width - (wide.a.x + wide.a.width)).toBeLessThan(
			16,
		);
		expect(
			Math.abs(wide.a.y + wide.a.height / 2 - (wide.r.y + wide.r.height / 2)),
		).toBeLessThanOrEqual(1);
		await narrow(page);
		const phone = await box();
		expect(phone.a.height).toBeGreaterThanOrEqual(44);
		expect(phone.a.width).toBeGreaterThanOrEqual(0.9 * phone.r.width);
		expect(phone.a.y).toBeGreaterThanOrEqual(
			phone.words.y + phone.words.height,
		);
		await page.setViewportSize({ width: 1280, height: 800 });

		// The render before the shell trades its rail for the bars, held: the shell is told the
		// window is wide at a phone's width. The user testing found the page 133 px too wide there.
		const stale = await page.context().newPage();
		await stale.addInitScript(() => {
			const real = window.matchMedia.bind(window);
			window.matchMedia = (query: string) =>
				query === "(min-width: 1024px)"
					? ({
							matches: true,
							media: query,
							onchange: null,
							addEventListener() {},
							removeEventListener() {},
							addListener() {},
							removeListener() {},
							dispatchEvent: () => false,
						} as MediaQueryList)
					: real(query);
		});
		await stale.setViewportSize({ width: 360, height: 780 });
		await stale.goto(new URL("/", page.url()).href);
		await expect(stale.getByText("Mira has a question")).toBeVisible();
		expect(
			await stale.evaluate(() => document.documentElement.scrollWidth),
		).toBeLessThanOrEqual(360);
		await stale.close();
		await page.getByRole("link", { name: "Answer" }).click();

		await expect(page).toHaveURL(/\/tasks\/FRK-1\/questions$/);
		await expect(
			page.getByRole("heading", { name: "Mira has a question" }),
		).toBeVisible();
		await expect(page.getByText("What should done.txt say?")).toBeVisible();
		await expect(
			page.getByText("The file only shows that the run finished."),
		).toBeVisible();
		await screenshots(page, "question");
		await page.getByRole("radio", { name: /Leave it empty/ }).check();
		await page.getByRole("button", { name: "Send answer" }).click();
		await expect
			.poll(
				() =>
					events(serve.project).find((e) => e.kind === "question.answered")
						?.body.answer,
			)
			.toBe("Leave it empty");

		// A low-risk plan needs no approval: once Ada has checked it, it waits to be picked up.
		await page.goto(`http://127.0.0.1:${serve.port}/requests/FRK-1`);
		await expect(page.getByText("To do", { exact: true })).toBeVisible({
			timeout: 15_000,
		});
	} finally {
		await serve.stop();
	}
});
