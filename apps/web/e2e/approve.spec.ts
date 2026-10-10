import { expect, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

test("a plan is read, edited with a live check, saved back to refining, then approved", async ({
	page,
}) => {
	const serve = await startServe({
		team: "pm-architect-developer",
		transcripts: [
			"triage_ctv_1_large",
			"refine_writes_epic_ctv_1",
			"judge_ctv_1_by_architect",
			"refine_writes_epic_ctv_1",
			"judge_ctv_1_by_architect",
		],
	});
	try {
		await page.goto(serve.url);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		await expect(page.getByText("Mira").first()).toBeVisible();
		await page
			.getByLabel("What should the team do next?")
			.fill("Add a done.txt at the root, and a check that it is there");
		await page.getByRole("button", { name: "Send to the team" }).click();
		await expect(page).toHaveURL(/\/requests\/CTV-1$/);
		await expect(
			page.getByText("Mira sized it as a big request"),
		).toBeVisible();

		await page.getByRole("link", { name: "Today" }).first().click();
		await expect(page.getByText(/^Approve the plan for /)).toBeVisible();
		await page.getByRole("link", { name: "Review" }).click();

		await expect(page).toHaveURL(/\/tasks\/CTV-1\/plan$/);
		await expect(
			page.getByText(
				"A done.txt file at the root of your project, so that anyone can see the run finished. Catervas checks that the file is there.",
				{ exact: true },
			),
		).toBeVisible();
		await expect(
			page.getByText("Mira, your Product Manager, wrote this for you"),
		).toBeVisible();
		await expect(
			page.getByRole("heading", { name: "What Catervas checked" }),
		).toBeVisible();
		await screenshots(page, "plan");

		await page.getByRole("link", { name: "Edit the plan yourself" }).click();
		await expect(page).toHaveURL(/\/tasks\/CTV-1\/plan\/edit$/);
		const verdict = page
			.getByRole("region", { name: "Ready to approve?" })
			.getByText(/ checks pass\./);
		const intent = page.getByLabel(/^Why you want it/);
		await expect(verdict).toHaveText(/^(\d+) of \1 checks pass\.$/);
		const passing = await verdict.textContent();
		// An empty intent fails a check once Catervas has read it; a new one passes again.
		await intent.fill("");
		await expect(verdict).toHaveText(
			/^0 of 1 checks pass\. The one left: .*shorter than 20 characters$/,
		);
		await intent.fill(
			"The repository has a done.txt at its root, so that anyone can see a run finished.",
		);
		await expect(verdict).toHaveText(passing ?? "");
		await screenshots(page, "plan-editor");
		await page.getByRole("button", { name: "Save" }).click();
		await expect(
			page.getByText(
				"Saved. Mira checks the plan again, then it comes back to you to approve.",
			),
		).toBeVisible();

		// Mira writes the plan again and Ada checks it; then it waits on the user once more.
		await page.goto(`http://127.0.0.1:${serve.port}/tasks/CTV-1/plan`);
		const approve = page.getByRole("button", { name: "Approve the plan" });
		await expect(approve).toBeVisible({ timeout: 15_000 });
		const judged = events(serve.project).filter(
			(e) => e.kind === "contract.judged",
		);
		expect(judged).toHaveLength(2);
		await approve.click();
		await expect(page).toHaveURL(/:\d+\/$/);
		await expect
			.poll(() =>
				events(serve.project)
					.filter((e) => e.kind === "human.accepted")
					.map((e) => e.body.subject),
			)
			.toEqual(["contract"]);
	} finally {
		await serve.stop();
	}
});
