import { expect, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

test("a big request's plan goes ahead without approval, and is still there to read", async ({
	page,
}) => {
	const serve = await startServe({
		team: "pm-architect-developer",
		transcripts: [
			"triage_ctv_1_large",
			"refine_writes_epic_ctv_1",
			"plan_breaks_down_ctv_1",
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

		// The plan is ready and broken into tasks with no one's approval.
		await expect
			.poll(
				() =>
					events(serve.project).filter(
						(e) => e.kind === "task.created" && e.task_id === "CTV-2",
					).length,
				{ timeout: 40_000 },
			)
			.toBe(1);
		const log = events(serve.project);
		expect(log.filter((e) => e.kind === "human.accepted")).toHaveLength(0);
		expect(
			log.some(
				(e) =>
					e.task_id === "CTV-1" &&
					e.kind === "task.transitioned" &&
					e.body.to === "escalated",
			),
		).toBe(false);
		await page.goto(`http://127.0.0.1:${serve.port}/`);
		await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
		await expect(page.getByText(/^Approve the plan for /)).toHaveCount(0);

		// The owner can still read the plan, but there is nothing to approve in it.
		await page.goto(`http://127.0.0.1:${serve.port}/tasks/CTV-1/plan`);
		await expect(
			page.getByText(
				"A done.txt file at the root of your project, so that anyone can see the run finished. Catervas checks that the file is there.",
				{ exact: true },
			),
		).toBeVisible();
		await expect(
			page.getByRole("heading", {
				name: "The plan for Add a done.txt at the root, and a check that it is there",
			}),
		).toBeVisible();
		await expect(
			page.getByText("Mira, your Product Manager, wrote this for you"),
		).toBeVisible();
		await expect(
			page.getByRole("heading", { name: "What Catervas checked" }),
		).toBeVisible();
		await expect(
			page.getByRole("button", { name: "Approve the plan" }),
		).toHaveCount(0);
		await expect(
			page.getByText(/^This does not wait on you now\./),
		).toBeVisible();
		await screenshots(page, "plan");
	} finally {
		await serve.stop();
	}
});
