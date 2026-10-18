import { expect, type Page, test } from "@playwright/test";
import {
	events,
	mergeByHand,
	startServe,
	taskBranch,
} from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

/** No page the journey visits says a word about sprints. */
const noSprint = async (page: Page) =>
	expect(await page.locator("body").innerText()).not.toMatch(/sprint/i);

/** The lanes, as the board names their headings (`lane-<lane>-title`), in the order a task moves. */
const LANES = ["planning", "todo", "in_progress", "stuck", "review", "done"];

test("work runs on the board by itself, from its plan to the team's look back", async ({
	page,
}) => {
	// Eight paced sessions and four pages photographed twice outlast the default 30 seconds.
	test.setTimeout(90_000);
	const serve = await startServe({
		team: "pm-architect-developer",
		// Each session waits before it plays, so the board redraws in every lane the task passes.
		paceMs: 600,
		transcripts: [
			"triage_ctv_1_small_by_pm",
			"refine_writes_task_for_theo_ctv_1",
			"plan_assigns_ctv_1_to_theo",
			"implement_finishes_ctv_1",
			"review_writes_note",
			"accept_ctv_1",
			"review",
			"retro",
		],
	});
	const at = (path: string) => `http://127.0.0.1:${serve.port}${path}`;
	try {
		// 1. No sprint anywhere: ready work would show under "To do".
		await page.goto(serve.url);
		await page.getByRole("link", { name: "Board" }).first().click();
		await expect(page.getByRole("region", { name: "To do" })).toBeVisible();
		await expect(page.getByRole("button", { name: /sprint/i })).toHaveCount(0);
		await noSprint(page);

		// 2. The request, filed on Today.
		await page.getByRole("link", { name: "Today" }).first().click();
		await page
			.getByLabel("What should the team do next?")
			.fill("Add a done.txt at the root, so a run can be checked for it");
		await page.getByRole("button", { name: "Send to the team" }).click();
		await expect(page).toHaveURL(/\/requests\/CTV-1$/);

		// 3. On the board, CTV-1 moves through the lanes by itself: every lane it is drawn in is
		// recorded as the board redraws.
		await page.getByRole("link", { name: "Board" }).first().click();
		await page.evaluate((lanes) => {
			const seen: string[] = [];
			(window as unknown as { lanesSeen: string[] }).lanesSeen = seen;
			const look = () => {
				for (const lane of lanes) {
					const column = document.querySelector(
						`section[aria-labelledby="lane-${lane}-title"]`,
					);
					if (
						column?.querySelector('a[href="/tasks/CTV-1"]') &&
						seen.at(-1) !== lane
					)
						seen.push(lane);
				}
			};
			new MutationObserver(look).observe(document.body, {
				childList: true,
				subtree: true,
				characterData: true,
			});
			look();
		}, LANES);
		const done = page.getByRole("region", { name: "Done" });
		const task = done.getByRole("link", {
			name: "Add a done.txt at the root, so a run can be checked for it",
		});
		await expect(task).toBeVisible({ timeout: 30_000 });
		expect(
			await page.evaluate(
				() => (window as unknown as { lanesSeen: string[] }).lanesSeen,
			),
		).toEqual(["planning", "todo", "in_progress", "review", "done"]);
		// Catervas never merges: accepted, it waits in Done for the owner's merge.
		await expect(done.getByText("Waiting on you")).toBeVisible();
		await screenshots(page, "board");

		// 4. The task's five tabs, each with what the run left.
		await task.click();
		await expect(page).toHaveURL(/\/tasks\/CTV-1$/);
		await expect(
			page.getByText("CTV-1. Theo is doing it, and Ada reviews it.", {
				exact: true,
			}),
		).toBeVisible();
		const panel = page.getByRole("tabpanel");
		await expect(
			panel.getByText(
				"The repository has a done.txt at its root, so that a run can be checked for it.",
			),
		).toBeVisible();
		await panel.getByText("Show each check").click();
		await expect(panel.getByText("Passed", { exact: true })).toBeVisible();
		await page.getByRole("tab", { name: "History" }).click();
		await expect(panel.getByText("Mira moved it to Done.")).toBeVisible();
		await expect(
			panel.getByText("Theo moved it to In progress."),
		).toBeVisible();
		await page.getByRole("tab", { name: "The plan" }).click();
		await expect(panel.getByText("$5.00 for this task")).toBeVisible();
		await page.getByRole("tab", { name: "Code changes" }).click();
		await expect(
			panel.getByText(
				`1 file, +0 −0, on the branch ${taskBranch(serve.project, "CTV-1")}.`,
			),
		).toBeVisible();
		await page.getByRole("tab", { name: "Notes" }).click();
		await expect(
			panel.getByText("Theo, your Developer, wrote this for you"),
		).toBeVisible();
		await expect(
			panel.getByText(
				"I added done.txt at the root of the project, as the plan asked. Nothing was left out.",
			),
		).toBeVisible();
		await expect(
			panel.getByText("Ada, your Architect, reviewed it"),
		).toBeVisible();
		const cost = page.getByRole("region", { name: "Cost so far" });
		await expect(cost.getByRole("row", { name: /^Building/ })).toBeVisible();
		// The owner merges the branch on their own checkout and adds the task.
		mergeByHand(serve.project, taskBranch(serve.project, "CTV-1"));
		await page.getByRole("button", { name: "Add to the project" }).click();
		await expect(page.getByText(/^Added on /)).toBeVisible({
			timeout: 15_000,
		});
		await screenshots(page, "task");
		await page.goto(at("/board"));
		await expect(
			page.getByRole("region", { name: "Done" }).getByText("Accepted"),
		).toBeVisible();

		// 5. The team's quiet closed the delivery by itself, and its review and look back ran.
		await expect
			.poll(
				() =>
					events(serve.project).filter((e) => e.kind === "retro.appended")
						.length,
			)
			.toBe(1);
		const closed = events(serve.project).filter(
			(e) => e.kind === "delivery.closed",
		);
		expect(closed).toHaveLength(1);
		expect(closed[0]?.body).toMatchObject({
			trigger: "team_quiet",
			task_ids: ["CTV-1"],
		});
		await page.goto(at("/channel"));
		await expect(
			page.getByRole("button", { name: /^Review, / }).first(),
		).toBeVisible();
		await expect(
			page.getByRole("button", { name: /^Looking back, / }).first(),
		).toBeVisible();
		await noSprint(page);
		await page.getByRole("link", { name: "Board" }).first().click();
		await noSprint(page);

		// 6. The Costs page's row for each agent, with what Theo spent building.
		await page.goto(at("/costs"));
		const theo = page.getByRole("row", { name: /^Theo/ });
		await expect(theo.getByRole("rowheader")).toHaveText(/^Theo/);
		await expect(theo.getByRole("cell")).toHaveText([
			"$0.03",
			"Nothing to do right now",
		]);
		await expect(page.getByText("1 of 1", { exact: true })).toBeVisible();
		await noSprint(page);
		// On a phone no amount, and no one-word column name, breaks in the middle.
		await page.setViewportSize({ width: 360, height: 780 });
		await expect
			.poll(() =>
				page.evaluate(() =>
					Array.from(
						document.querySelectorAll(
							"table thead th, table td:not(:last-child)",
						),
					)
						.filter((cell) => {
							if (/\s/.test(cell.textContent?.trim() ?? "")) return false;
							const range = document.createRange();
							range.selectNodeContents(cell);
							return (
								new Set(
									Array.from(range.getClientRects()).map((r) =>
										Math.round(r.top),
									),
								).size > 1
							);
						})
						.map((cell) => cell.textContent),
				),
			)
			.toEqual([]);
		await screenshots(page, "costs");

		// 7. Settings' team rules, on a running team.
		await page.goto(at("/settings/work"));
		await expect(
			page.getByRole("region", { name: "How finished work is added" }),
		).toBeVisible();
		await noSprint(page);
		await screenshots(page, "team-rules");
	} finally {
		await serve.stop();
	}
});
