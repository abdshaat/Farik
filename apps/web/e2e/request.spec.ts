import { expect, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

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
