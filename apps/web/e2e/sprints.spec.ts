import { expect, type Page, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { narrow, screenshots } from "./fixtures/shots.ts";

/** The sprints' screenshots are taken at 1440, with the Backlog lane beside the others. */
const shots = (page: Page, name: string) => screenshots(page, name, 1440);

/** The Board photographed, on its Backlog tab at a phone's width. */
async function backlogShots(page: Page, name: string) {
	await narrow(page);
	await page.getByRole("button", { name: /^Backlog \d+$/ }).click();
	await shots(page, name);
}

/** Files `text` from Today's request box. */
async function file(page: Page, text: string) {
	await page.getByRole("link", { name: "Today" }).first().click();
	await page.getByLabel("What should the team do next?").fill(text);
	await page.getByRole("button", { name: "Send to the team" }).click();
}

/** Answers the question Today lists from `who` with "Leave it empty". */
async function answer(page: Page, who: string) {
	await page.getByRole("link", { name: "Today" }).first().click();
	await expect(page.getByText(`${who} has a question`)).toBeVisible({
		timeout: 15_000,
	});
	await page.getByRole("link", { name: "Answer" }).click();
	await page.getByRole("radio", { name: /Leave it empty/ }).check();
	await page.getByRole("button", { name: "Send answer" }).click();
}

test("ready work waits in the backlog until a sprint plans it, and late work for the next one, through the real server and browser", async ({
	page,
}) => {
	// Eighteen sessions, four pages photographed twice, and two three-second watches.
	test.setTimeout(180_000);
	const serve = await startServe({
		team: "pm-architect-developer",
		sprints: true,
		transcripts: [
			// FRK-1, the small request: Mira sizes it and asks, then writes it; Ada checks it.
			"triage_frk_1_small_by_pm",
			"ask_with_choices_frk_1",
			"refine_writes_task_for_theo_frk_1",
			"judge_frk_1_by_architect",
			// FRK-2, the epic: sized, written, checked, approved by the user, then broken down
			// outside a sprint into FRK-3, which Ada checks.
			"triage_frk_1_large",
			"refine_writes_epic_frk_1",
			"judge_frk_1_by_architect",
			"plan_breaks_down_frk_2",
			"judge_frk_1_by_architect",
			// Sprint 1: planned, FRK-1 handed to Theo, and Theo asks the user before he builds.
			"planning_ceremony_frk_1_frk_2",
			"plan_assigns_frk_1_to_theo",
			"ask_with_choices_frk_1",
			// FRK-4, the late request, ready during sprint 1.
			"triage_frk_1_small_by_pm",
			"refine_writes_task_for_theo_frk_1",
			"judge_frk_1_by_architect",
			// Sprint 1 ended early: its review and look back; then, with the policy off, Theo builds.
			"review",
			"retro",
			"implement_finishes_frk_1",
		],
	});
	const at = (path: string) => `http://127.0.0.1:${serve.port}${path}`;
	const log = () => events(serve.project);
	const moves = (id: string) =>
		log()
			.filter((e) => e.kind === "task.transitioned" && e.task_id === id)
			.map((e) => `${e.body.from} -> ${e.body.to}`);
	const assigned = () =>
		log()
			.filter((e) => e.kind === "task.transitioned" && e.body.to === "assigned")
			.map((e) => e.task_id);
	const backlog = page.getByRole("region", { name: "Backlog" });
	try {
		await page.goto(serve.url);

		// 1. The small request: Mira asks, the user answers, and Ada checks the plan.
		await file(
			page,
			"Add a done.txt at the root, so a run can be checked for it",
		);
		await expect(page).toHaveURL(/\/requests\/FRK-1$/);
		await answer(page, "Mira");
		await expect
			.poll(() => moves("FRK-1"), { timeout: 30_000 })
			.toContain("refining -> ready");

		// 2. The epic: the user approves its plan, and Mira breaks it down outside a sprint.
		await file(
			page,
			"Add a done.txt at the root, and a check that it is there",
		);
		await expect(page).toHaveURL(/\/requests\/FRK-2$/);
		await page.getByRole("link", { name: "Today" }).first().click();
		await expect(page.getByText(/^Approve the plan for /)).toBeVisible({
			timeout: 30_000,
		});
		await page.getByRole("link", { name: "Review" }).click();
		await expect(page).toHaveURL(/\/tasks\/FRK-2\/plan$/);
		await page.getByRole("button", { name: "Approve the plan" }).click();
		await expect
			.poll(() => moves("FRK-3"), { timeout: 30_000 })
			.toContain("refining -> ready");

		// 3. Both wait in the Backlog, and no task is handed to anyone: only the epic, to Mira,
		// whose breakdown is preparation.
		await page.goto(at("/"));
		await expect(
			page.getByText("2 pieces of work are ready and wait in the Backlog."),
		).toBeVisible();
		await shots(page, "sprints-today");
		await page.waitForTimeout(3000);
		expect(assigned()).toEqual(["FRK-2"]);
		expect(
			log().find(
				(e) => e.kind === "task.transitioned" && e.body.to === "assigned",
			)?.body.assignee,
		).toBe("mira");
		expect(log().filter((e) => e.kind === "sprint.started")).toEqual([]);
		await page.getByRole("link", { name: "Board" }).first().click();
		await expect(
			page.getByText(
				"No sprint is running. Ready work waits in the Backlog until you start one.",
			),
		).toBeVisible();
		await expect(backlog.locator('a[href="/tasks/FRK-1"]')).toBeVisible();
		await expect(backlog.locator('a[href="/tasks/FRK-2"]')).toBeVisible();
		await expect(backlog.getByText("Ready, broken into 1 task")).toBeVisible();
		await expect(backlog.getByText("1 part, 0 done")).toBeVisible();
		await backlogShots(page, "sprints-backlog");

		// 4. Today's link opens the start dialog on the Board, which lists both.
		await page.getByRole("link", { name: "Today" }).first().click();
		await page.getByRole("link", { name: "Start a sprint" }).click();
		await expect(page).toHaveURL(/\/board\?start=sprint$/);
		const start = page.getByRole("dialog", { name: "Start sprint 1" });
		const waiting = start.getByRole("list", { name: "Waiting in the Backlog" });
		await expect(waiting.getByRole("listitem")).toHaveText([
			/^FRK-1.*Task$/,
			/^FRK-2.*Epic, 1 task$/,
		]);
		await expect(start.getByLabel("No limit")).toBeChecked();
		await shots(page, "sprints-start");
		await start.getByRole("button", { name: "Start sprint 1" }).click();

		// 5. Planning takes both, the epic bringing its task, and only then is FRK-1 handed to Theo.
		await expect
			.poll(() => assigned(), { timeout: 30_000 })
			.toEqual(["FRK-2", "FRK-1"]);
		const kinds = log().map((e) => e.kind);
		const started = kinds.indexOf("sprint.started");
		const planned = log().findIndex((e) => e.kind === "sprint.planned");
		const handed = log().findIndex(
			(e) =>
				e.kind === "task.transitioned" &&
				e.task_id === "FRK-1" &&
				e.body.to === "assigned",
		);
		expect(started).toBeGreaterThan(-1);
		expect(started).toBeLessThan(planned);
		expect(planned).toBeLessThan(handed);
		expect(log()[planned]?.body).toMatchObject({
			sprint_id: "S1",
			task_ids: ["FRK-1", "FRK-2", "FRK-3"],
		});
		expect(log()[handed]?.body.assignee).toBe("theo");
		await expect(backlog.getByRole("link")).toHaveCount(0);
		// Theo asks before he builds, so the sprint holds still while the late request comes in.
		await expect
			.poll(() =>
				log().some(
					(e) => e.kind === "question.asked" && e.body.asked_by === "theo",
				),
			)
			.toBe(true);

		// 6. A request ready during sprint 1 waits for the next one.
		await file(page, "Add a NOTES.md at the root, with one line about the run");
		await expect(page).toHaveURL(/\/requests\/FRK-4$/);
		await expect
			.poll(() => moves("FRK-4"), { timeout: 30_000 })
			.toContain("refining -> ready");
		await page.goto(at("/"));
		await expect(
			page.getByText("1 more waits in the Backlog for the next sprint."),
		).toBeVisible();
		await page.goto(at("/board"));
		await expect(backlog.locator('a[href="/tasks/FRK-4"]')).toBeVisible();
		await expect(
			backlog.getByText("Ready. Waits for the next sprint"),
		).toBeVisible();
		await backlogShots(page, "sprints-late");
		await page.waitForTimeout(3000);
		expect(assigned()).toEqual(["FRK-2", "FRK-1"]);
		expect(
			log()
				.filter((e) => e.kind === "sprint.planned")
				.flatMap((e) => e.body.task_ids as string[]),
		).not.toContain("FRK-4");

		// 7. Ending sprint 1 early sends its unfinished work to the Backlog: FRK-1 stays where it
		// is, and even once Theo's question is answered no session builds it.
		await page.goto(at("/sprints/S1"));
		await page.getByRole("button", { name: "End the sprint early" }).click();
		const end = page.getByRole("dialog", { name: "End sprint 1 early?" });
		await expect(end).toContainText(
			"They leave the sprint and wait in the Backlog for the next one.",
		);
		await end.getByRole("button", { name: "End sprint 1" }).click();
		await expect
			.poll(() => log().find((e) => e.kind === "sprint.ended")?.body)
			.toMatchObject({
				sprint_id: "S1",
				ended_by: "human",
				backlog: true,
				left: ["FRK-1", "FRK-2", "FRK-3"],
			});
		await expect
			.poll(() => log().filter((e) => e.kind === "retro.appended").length, {
				timeout: 30_000,
			})
			.toBe(1);
		await answer(page, "Theo");
		await expect
			.poll(() => log().filter((e) => e.kind === "question.answered").length)
			.toBe(2);
		const sessions = () =>
			log().filter((e) => e.kind === "session.started" && e.task_id === "FRK-1")
				.length;
		const before = sessions();
		await page.waitForTimeout(3000);
		expect(sessions()).toBe(before);
		expect(moves("FRK-1").at(-1)).toBe("assigned -> in_progress");
		await page.goto(at("/board"));
		await expect(backlog.locator('a[href="/tasks/FRK-1"]')).toBeVisible();

		// 8. Switching the policy off in Settings releases it: Theo builds FRK-1.
		await page.goto(at("/settings"));
		const planning = page.getByRole("region", { name: "Planning work" });
		await planning.getByLabel("Plan work in sprints").click();
		await expect(
			planning.getByText(
				"Ready work starts as soon as someone is free, without waiting for a sprint.",
			),
		).toBeVisible();
		await planning.getByRole("button", { name: "Save" }).click();
		await expect
			.poll(() =>
				log()
					.filter((e) => e.kind === "team.updated")
					.map((e) => e.body.plan_in_sprints),
			)
			.toContain(false);
		await expect
			.poll(() => moves("FRK-1"), { timeout: 30_000 })
			.toContain("in_progress -> verifying");
		await page.goto(at("/board"));
		await expect(page.getByRole("region", { name: "Backlog" })).toHaveCount(0);
	} finally {
		await serve.stop();
	}
});
