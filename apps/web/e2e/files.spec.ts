import { expect, type Page, test } from "@playwright/test";
import { commitFiles, events, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

/** The Files page's screenshots are taken at 1440, as the chats' are. */
const shots = (page: Page, name: string) => screenshots(page, name, 1440);

const ROADMAP = "docs/catervas/product/roadmap.md";
const TWIN = "docs/catervas/product/roadmap.agent.md";
const OVERVIEW = "docs/catervas/architecture/overview.md";

test("the owner reads and edits the team's documents, through the real server and browser", async ({
	page,
}) => {
	const serve = await startServe({
		team: "pm-architect-developer",
		transcripts: [],
	});
	commitFiles(
		serve.project,
		{
			[ROADMAP]: "# Roadmap\n\nPie pre-orders.\n",
			[TWIN]: "# Roadmap\n\nNow: pie pre-orders.\n",
			[OVERVIEW]:
				"# Overview\n\n```mermaid\nflowchart LR\n  Shop --> Orders\n```\n",
		},
		"docs: the team's documents",
	);
	const at = (path: string) => `http://127.0.0.1:${serve.port}${path}`;
	try {
		await page.goto(serve.url);

		// 1. The rail's Files: Product (Mira), Architecture (Ada), Engineering (Theo) with nothing yet.
		await page
			.getByRole("navigation", { name: "Main" })
			.getByRole("link", { name: "Files" })
			.click();
		const tree = page.getByRole("navigation", { name: "Files" });
		await expect(tree.getByText("Mira")).toBeVisible();
		await expect(tree.getByText("Ada")).toBeVisible();
		await expect(tree.getByText("Theo")).toBeVisible();
		await expect(tree.getByText("Product", { exact: true })).toBeVisible();
		await expect(tree.getByText("Architecture", { exact: true })).toBeVisible();
		await expect(tree.getByText("Nothing here yet")).toBeVisible();
		await expect(
			page.getByRole("heading", { name: "Roadmap", level: 2 }),
		).toBeVisible();

		// 2. Ada's overview, its diagram drawn as an image under the served policy.
		await tree.getByText("Architecture", { exact: true }).click();
		await tree.getByRole("link", { name: "Overview" }).click();
		const diagram = page.getByRole("img", { name: "Diagram" });
		await expect(diagram).toBeVisible();
		await expect
			.poll(() => diagram.evaluate((img: HTMLImageElement) => img.naturalWidth))
			.toBeGreaterThan(0);
		await shots(page, "files");

		// 3. The roadmap's version for the team.
		await page.goto(at("/files/product/roadmap.md"));
		await page.getByRole("button", { name: "For the team" }).click();
		await expect(page.getByText("Now: pie pre-orders.")).toBeVisible();
		await page.getByRole("button", { name: "For you" }).click();

		// 4. Edit, a new line, Save: the owner's change, then Mira brings the team's version up to date.
		await page.getByRole("button", { name: "Edit" }).click();
		await expect(
			page.getByText(
				"Saving keeps your change at once. Mira will update the team’s version.",
			),
		).toBeVisible();
		const box = page.getByLabel("Roadmap text");
		await box.fill(`${await box.inputValue()}\nGift cards next.\n`);
		await shots(page, "files-edit");
		await page.getByRole("button", { name: "Save" }).click();
		await expect(page.getByText("Gift cards next.")).toBeVisible();
		await expect(page.getByText("Changed by you just now")).toBeVisible({
			timeout: 15_000,
		});
		const log = events(serve.project);
		expect(
			log.filter(
				(e) => e.kind === "folder_doc.edited" && e.body.path === ROADMAP,
			),
		).toHaveLength(1);
		expect(
			log.filter((e) => e.kind === "folder_change.integrated"),
		).toHaveLength(1);
		expect(
			log.filter((e) => e.kind === "task.created" && e.body.rederives === TWIN),
		).toHaveLength(1);
		await page.getByRole("button", { name: "For the team" }).click();
		await expect(
			page.getByText("Mira is bringing this up to date with your change."),
		).toBeVisible();
		await shots(page, "files-stale");
		await page.getByRole("button", { name: "For you" }).click();

		// 5. The file changed on the owner's computer while they edited: their text stays.
		await page.getByRole("button", { name: "Edit" }).click();
		const again = page.getByLabel("Roadmap text");
		const typed = `${await again.inputValue()}\nA line I typed.\n`;
		await again.fill(typed);
		commitFiles(
			serve.project,
			{ [ROADMAP]: "# Roadmap\n\nChanged on your computer.\n" },
			"docs: change the roadmap",
		);
		await page.getByRole("button", { name: "Save" }).click();
		await expect(
			page
				.getByRole("alert")
				.getByText(
					"This file changed while you were editing. Your text is still here; copy it, then reload to see the new version.",
				),
		).toBeVisible();
		await expect(again).toHaveValue(typed);
		await shots(page, "files-changed");
		await page.getByRole("button", { name: "Reload" }).click();
		await expect(again).toHaveValue("# Roadmap\n\nChanged on your computer.\n");

		// 6. On a phone, from Team, the picker reaches Ada's overview.
		await page.setViewportSize({ width: 390, height: 844 });
		await page.goto(at("/team"));
		await page.getByRole("link", { name: "Files" }).click();
		await page
			.getByLabel("File")
			.selectOption({ label: "Architecture › Overview" });
		await expect(page).toHaveURL(/\/files\/architecture\/overview\.md$/);
		await expect(page.getByRole("img", { name: "Diagram" })).toBeVisible();
		await shots(page, "files-phone");
	} finally {
		await serve.stop();
	}
});
