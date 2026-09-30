import { expect, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

/** The lanes, as the board names their headings (`lane-<lane>-title`), in the order a task moves. */
const LANES = ["planning", "todo", "in_progress", "stuck", "review", "done"];

test("a sprint runs on the board by itself, from its planning to its look back", async ({
	page,
}) => {
	// Ten paced sessions and five pages photographed twice outlast the default 30 seconds.
	test.setTimeout(90_000);
	const serve = await startServe({
		team: "pm-architect-developer",
		// Each session waits before it plays, so the board redraws in every lane the task passes.
		paceMs: 600,
		transcripts: [
			"triage_frk_1_small_by_pm",
			"refine_writes_task_for_theo_frk_1",
			"judge_frk_1_by_architect",
			"planning_ceremony_frk_1",
			"plan_assigns_frk_1_to_theo",
			"implement_finishes_frk_1",
			"review_writes_note",
			"accept_frk_1",
			"review",
			"retro",
		],
	});
	const at = (path: string) => `http://127.0.0.1:${serve.port}${path}`;
	try {
		// 1. A sprint with a $20 budget; the board's sprint line opens its page.
		await page.goto(serve.url);
		await page.getByRole("link", { name: "Board" }).first().click();
		await expect(
			page.getByText(
				"No sprint is running. The team works through the board in order.",
			),
		).toBeVisible();
		await page.getByRole("button", { name: "Start a sprint" }).click();
		const start = page.getByRole("dialog", { name: "Start sprint 1" });
		await start.getByLabel("Stop handing out new work after").check();
		await expect(start.getByLabel("Dollars")).toHaveValue("20.00");
		await start.getByRole("button", { name: "Start sprint 1" }).click();
		await page
			.getByRole("link", { name: "Sprint 1 is running: 0 of 0 tasks done" })
			.click();
		await expect(page).toHaveURL(/\/sprints\/S1$/);
		await expect(
			page.getByText("$0.00 so far, of the $20.00 you set for this sprint."),
		).toBeVisible();
		expect(
			events(serve.project).find((e) => e.kind === "sprint.started")?.body,
		).toMatchObject({ sprint_id: "S1", budget_usd: 20, started_by: "human" });

		// 2. The request, filed on Today once the sprint is open, so its planning plans it.
		await page.getByRole("link", { name: "Today" }).first().click();
		await page
			.getByLabel("What should the team do next?")
			.fill("Add a done.txt at the root, so a run can be checked for it");
		await page.getByRole("button", { name: "Send to the team" }).click();
		await expect(page).toHaveURL(/\/requests\/FRK-1$/);

		// 3. On the board, FRK-1 moves through the lanes by itself: every lane it is drawn in is
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
						column?.querySelector('a[href="/tasks/FRK-1"]') &&
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
		await expect(done.getByText("Accepted")).toBeVisible();
		await screenshots(page, "board");

		// 4. The task's five tabs, each with what the run left.
		await task.click();
		await expect(page).toHaveURL(/\/tasks\/FRK-1$/);
		await expect(
			page.getByText(
				"FRK-1 in sprint 1. Theo is doing it, and Ada reviews it. Try 1 of 4.",
			),
		).toBeVisible();
		const panel = page.getByRole("tabpanel");
		await expect(
			panel.getByText(
				"The repository has a done.txt at its root, so that a run can be checked for it.",
			),
		).toBeVisible();
		await expect(panel.getByText("Passed")).toBeVisible();
		await page.getByRole("tab", { name: "History" }).click();
		await expect(panel.getByText("Mira moved it to Done.")).toBeVisible();
		await expect(
			panel.getByText("Theo moved it to In progress."),
		).toBeVisible();
		await page.getByRole("tab", { name: "The plan" }).click();
		await expect(panel.getByText("$5.00 for this task")).toBeVisible();
		await page.getByRole("tab", { name: "Code changes" }).click();
		await expect(
			panel.getByText("1 file, +0 −0, on the branch feature/FRK-1."),
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
		await expect(page.getByText(/^Added on /)).toBeVisible({
			timeout: 15_000,
		});
		await screenshots(page, "task");

		// 5. The sprint ended by itself once its one task was accepted, and its meetings are listed.
		await expect
			.poll(
				() =>
					events(serve.project).filter((e) => e.kind === "retro.appended")
						.length,
			)
			.toBe(1);
		expect(
			events(serve.project).find((e) => e.kind === "sprint.ended")?.body,
		).toMatchObject({ sprint_id: "S1", ended_by: "governor" });
		await page.getByRole("link", { name: "Board" }).first().click();
		await expect(
			page.getByText(
				"No sprint is running. The team works through the board in order.",
			),
		).toBeVisible();
		await page.goto(at("/sprints/S1"));
		await expect(page.getByText(/Planned by Mira\. Ended /)).toBeVisible();
		await expect(page.getByText(/^1 of 1 done\./)).toBeVisible();
		const meetings = page.getByRole("list", { name: "Team meetings" });
		await expect(meetings.getByRole("listitem")).toHaveText([
			/^Planning2 posts on /,
			/^Review1 post on /,
			/^Looking back1 post on /,
		]);
		await screenshots(page, "sprint");

		// 6. The Costs page's row for each agent, with what Theo spent building.
		await page.getByRole("link", { name: "See costs by agent" }).click();
		await expect(page).toHaveURL(/\/costs$/);
		const theo = page.getByRole("row", { name: /^Theo/ });
		await expect(theo.getByRole("rowheader")).toHaveText(/^Theo/);
		await expect(theo.getByRole("cell")).toHaveText([
			"$0.04",
			"$0.00",
			"Nothing to do right now",
		]);
		await expect(page.getByText("1 of 1", { exact: true })).toBeVisible();
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
		await page.goto(at("/settings"));
		await expect(
			page.getByRole("region", { name: "How finished work is added" }),
		).toBeVisible();
		await screenshots(page, "team-rules");
	} finally {
		await serve.stop();
	}
});
