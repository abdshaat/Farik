import { existsSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

/** The kinds of `task`'s events, each with the agent that recorded it, in the log's order. */
function logOf(project: string, task: string): string[] {
	return events(project)
		.filter((event) => event.task_id === task)
		.map((event) => `${event.kind} ${event.agent_id ?? ""}`.trim());
}

async function send(page: Page, request: string, task: string) {
	await page.getByRole("link", { name: "Today" }).first().click();
	await page.getByLabel("What should the team do next?").fill(request);
	await page.getByRole("button", { name: "Send to the team" }).click();
	await expect(page).toHaveURL(new RegExp(`/requests/${task}$`));
}

test("a Designer's task and a design review run through the real server and browser", async ({
	page,
}) => {
	// Six page checks run Chromium in Docker, and each preview starts in a container of its own.
	test.setTimeout(300_000);
	const serve = await startServe({
		team: "pm-architect-developer-designer",
		transcripts: [
			"triage_frk_1_small_by_pm",
			"refine_writes_page_task_for_iris_frk_1",
			"plan_assigns_frk_1_to_iris",
			"explore_checks_and_plans_frk_1",
			"decide_design_plan_approves_frk_1",
			"implement_by_iris_page_frk_1",
			"review_answers_the_rubric_frk_1",
			"accept_frk_1",
			"triage_frk_2_small_by_pm",
			"refine_writes_css_task_for_theo_frk_2",
			"plan_assigns_frk_2_to_theo",
			"implement_css_frk_2",
			"design_review_passes_frk_2",
			"review_answers_the_rubric_frk_2",
		],
	});
	try {
		await page.goto(serve.url);

		// 1. The request is triaged, refined for Iris, and assigned to her.
		await send(page, "Make the sign-in page say where to sign in", "FRK-1");
		await expect
			.poll(() => logOf(serve.project, "FRK-1"), { timeout: 30_000 })
			.toContainEqual(expect.stringMatching(/^task\.transitioned/));

		// 2. Iris explores, and her page check really runs on the Playwright image.
		await expect
			.poll(
				() =>
					events(serve.project).filter(
						(e) => e.task_id === "FRK-1" && e.kind === "page.checked",
					),
				{ timeout: 120_000 },
			)
			.toHaveLength(1);
		const [explored] = events(serve.project).filter(
			(e) => e.kind === "page.checked",
		);
		expect(explored?.agent_id).toBe("iris");
		expect(explored?.body.width).toBe("phone");
		const shot = String(explored?.body.screenshot);
		expect(
			existsSync(join(serve.project, ".farik/local/screenshots/FRK-1", shot)),
		).toBe(true);

		// 3 and 4. The plan waits for Mira and is approved; Iris implements, Ada reviews, and it is
		// accepted.
		await expect
			.poll(() => logOf(serve.project, "FRK-1"), { timeout: 120_000 })
			.toContainEqual("review.recorded ada");
		const log = logOf(serve.project, "FRK-1");
		const at = (entry: string) => log.indexOf(entry);
		expect(at("design_plan.proposed iris")).toBeGreaterThan(-1);
		expect(at("design_plan.approved mira")).toBeGreaterThan(
			at("design_plan.proposed iris"),
		);
		expect(at("review.recorded ada")).toBeGreaterThan(
			at("design_plan.approved mira"),
		);
		await expect
			.poll(
				() =>
					events(serve.project).some(
						(e) =>
							e.task_id === "FRK-1" &&
							e.kind === "task.transitioned" &&
							e.body.to === "accepted",
					),
				{ timeout: 60_000 },
			)
			.toBe(true);

		// 5. Theo's change to site/style.css is checked in the browser before Ada reviews it.
		await send(page, "Darken the page's heading", "FRK-2");
		await page.getByRole("link", { name: "Today" }).first().click();
		await expect(page.getByText(/^Approve the plan for /)).toBeVisible({
			timeout: 30_000,
		});
		await page.getByRole("link", { name: "Review" }).click();
		await page.getByRole("button", { name: "Approve the plan" }).click();
		// "Checking the screens" is not asserted here: it shows only between Theo's commit and
		// Iris's review, a few seconds the fake models can finish before the page has loaded. The
		// task page and the daemon's task.get tests hold that wording.
		await expect
			.poll(() => logOf(serve.project, "FRK-2"), { timeout: 180_000 })
			.toContainEqual("review.recorded ada");
		const frk2 = logOf(serve.project, "FRK-2");
		const review = events(serve.project).find(
			(e) => e.task_id === "FRK-2" && e.kind === "design_review.recorded",
		);
		expect(review?.agent_id).toBe("iris");
		expect(review?.body.pass).toBe(true);
		expect(frk2.filter((entry) => entry === "page.checked iris")).toHaveLength(
			4,
		);
		expect(frk2.indexOf("review.recorded ada")).toBeGreaterThan(
			frk2.indexOf("design_review.recorded iris"),
		);

		// 6. The team, the task page and the gate, at both widths.
		await page.getByRole("link", { name: "Team" }).first().click();
		await expect(page.getByText("Iris").first()).toBeVisible();
		await screenshots(page, "designer-team");
		await page.goto(`http://127.0.0.1:${serve.port}/tasks/FRK-2`);
		await expect(
			page.getByText("Iris, your UI/UX Designer, checked the screens first"),
		).toBeVisible({ timeout: 20_000 });
		await screenshots(page, "designer-task");
		await page.goto(`http://127.0.0.1:${serve.port}/tasks/FRK-2/accept`);
		await expect(
			page.getByText("Iris, your UI/UX Designer, checked the screens first"),
		).toBeVisible({ timeout: 20_000 });
		await screenshots(page, "designer-gate");
	} finally {
		await serve.stop();
	}
});
