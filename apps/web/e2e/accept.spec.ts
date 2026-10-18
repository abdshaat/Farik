import { expect, test } from "@playwright/test";
import { events, mergeByHand, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

test("a high-risk request needs no approval, and its accepted work waits for the owner's merge", async ({
	page,
}) => {
	// Three sessions play before the task is accepted, and a slow disk stretches each one.
	test.setTimeout(90_000);
	const serve = await startServe({
		team: "pm-architect-developer",
		transcripts: [
			"triage_ctv_1_small_by_pm",
			"refine_writes_high_risk_ctv_1",
			"plan_assigns_ctv_1_to_theo",
			"implement_finishes_ctv_1",
			"review_writes_note",
			"accept_ctv_1",
		],
	});
	try {
		await page.goto(serve.url);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		await expect(page.getByText("Mira").first()).toBeVisible();
		await page
			.getByLabel("What should the team do next?")
			.fill("Add a done.txt at the root, so a run can be checked for it");
		await page.getByRole("button", { name: "Send to the team" }).click();
		await expect(page).toHaveURL(/\/requests\/CTV-1$/);

		// A high-risk plan waits for no one: Theo builds it and Ada reviews it by themselves.
		await expect
			.poll(
				() =>
					events(serve.project).some(
						(e) => e.kind === "task.transitioned" && e.body.to === "accepted",
					),
				{ timeout: 60_000 },
			)
			.toBe(true);
		expect(
			events(serve.project).filter((e) => e.kind === "human.accepted"),
		).toHaveLength(0);

		// Nothing is left to approve or accept; the only thing waiting is the merge, by hand.
		await page.goto(`http://127.0.0.1:${serve.port}/`);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		await expect(page.getByText(/^Approve the plan for /)).toHaveCount(0);
		await expect(page.getByText(/^Accept /)).toHaveCount(0);
		await expect(page.getByText(/^Add .* to your project$/)).toBeVisible();
		await expect(
			page.getByText("Accepted: waiting for you to merge feature/CTV-1", {
				exact: true,
			}),
		).toBeVisible();
		await screenshots(page, "gate");

		// The owner merges it on their own checkout, then adds it.
		mergeByHand(serve.project, "feature/CTV-1");
		await page.getByRole("link", { name: "Add", exact: true }).click();
		await expect(page).toHaveURL(/\/tasks\/CTV-1\/accept$/);
		await page.getByRole("button", { name: "Add to project" }).click();
		await expect
			.poll(
				() =>
					events(serve.project).filter((e) => e.kind === "task.integrated")
						.length,
				{ timeout: 30_000 },
			)
			.toBe(1);
	} finally {
		await serve.stop();
	}
});
