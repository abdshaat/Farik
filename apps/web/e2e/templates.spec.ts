import { copyFileSync, existsSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { expect, type Page, test } from "@playwright/test";
import { events, startServe, stateFolder } from "./fixtures/serve.ts";
import { narrow, screenshots } from "./fixtures/shots.ts";

const shots = (page: Page, name: string) =>
	screenshots(page, `templates-${name}`, 1440);
/** Setup's screens have no rail to wait for, so they are only resized. */
async function setupShots(page: Page, name: string) {
	const path = new URL(`./screenshots/templates-${name}`, import.meta.url)
		.pathname;
	await narrow(page);
	await page.screenshot({ path: `${path}-360.png`, fullPage: true });
	await page.setViewportSize({ width: 1440, height: 800 });
	await page.screenshot({ path: `${path}-1440.png`, fullPage: true });
}
const templates = join(stateFolder, "templates");
const mode = (path: string) => statSync(path).mode & 0o777;
const teamOf = (project: string) =>
	readFileSync(join(project, ".farik/team.yaml"), "utf8");
/** Each agent's id and status, in the team file's order. */
const statuses = (project: string) =>
	[
		...teamOf(project).matchAll(/^ {2}id: (\S+)[\s\S]*?^ {2}status: (\S+)/gm),
	].map(([, id, status]) => `${id} ${status}`);

test("the user saves a team, starts a project from it, and uses one on a live team, through the real server and browser", async ({
	page,
	browser,
}) => {
	const a = await startServe({
		team: "pm-architect-developer",
		transcripts: ["reply_to_a_mention"],
	});
	const b = await startServe({
		project: true,
		setupPending: true,
		transcripts: [],
	});
	// Each server's browser keeps its own sign-in, so B gets a browser of its own.
	const other = await browser.newContext();
	const pageB = await other.newPage();
	const inA = (path: string) => `http://127.0.0.1:${a.port}${path}`;
	try {
		// 1. Project A: Theo works, push is allowed, and the team is saved.
		await page.goto(a.url);
		await page.getByRole("link", { name: "Chats" }).first().click();
		const box = page.getByLabel("Post to the team");
		await box.fill("@theo can you look at the menu page?");
		await page.getByRole("button", { name: "Post" }).click();
		await expect(
			page
				.getByRole("list", { name: "Messages" })
				.getByText("the login form is done and its tests are next."),
		).toBeVisible({ timeout: 15_000 });

		await page.goto(inA("/settings"));
		const may = page.getByRole("region", { name: "What agents may do" });
		await may.getByLabel(/^Yes, on its own branches/).check();
		await may.getByRole("button", { name: "Save changes" }).click();
		await expect.poll(() => teamOf(a.project)).toMatch(/push: true/);

		await page.goto(inA("/team"));
		const before = events(a.project).length;
		await page.getByRole("button", { name: "Save as a template" }).click();
		await page.getByLabel("Name", { exact: true }).fill("Three of us");
		await page.getByRole("button", { name: "Save", exact: true }).click();
		await expect(page.getByRole("status")).toContainText(
			"Saved as Three of us.",
		);
		const saved = join(templates, "three-of-us.yaml");
		expect(mode(saved)).toBe(0o600);
		expect(mode(templates)).toBe(0o700);
		expect(readFileSync(saved, "utf8")).toMatch(/push: true/);
		expect(events(a.project)).toHaveLength(before);

		// 2. Project B: setup starts from the saved team, and its permission answers are not asked.
		await pageB.goto(b.url);
		await expect(pageB).toHaveURL(/\/setup\/scan$/);
		await pageB.getByRole("button", { name: "That's right" }).click();
		await expect(pageB).toHaveURL(/\/setup\/team$/);
		await pageB.getByLabel(/^A saved team/).check();
		await expect(pageB.getByLabel(/^Three of us/)).toBeChecked();
		const names = pageB
			.getByRole("list", { name: "Your team" })
			.getByRole("textbox");
		await expect(names).toHaveCount(3);
		for (const [i, name] of ["Mira", "Ada", "Theo"].entries())
			await expect(names.nth(i)).toHaveValue(name);
		await setupShots(pageB, "starts");
		await pageB.getByRole("button", { name: /^Continue/ }).click();
		await expect(pageB).toHaveURL(/\/setup\/spending$/);
		await pageB.getByRole("button", { name: "Continue", exact: true }).click();
		await expect(pageB).toHaveURL(/\/setup\/finish$/);
		const carried = pageB.getByRole("region", {
			name: "What they may do, from Three of us",
		});
		await expect(carried).toContainText(
			"The Developer may send work to its own branches online.",
		);
		await pageB.getByRole("button", { name: "Start the team" }).click();
		await expect(pageB).toHaveURL(/:\d+\/$/);
		const yamlB = teamOf(b.project);
		for (const name of ["Mira", "Ada", "Theo"]) {
			expect(yamlB).toMatch(new RegExp(`id: ${name.toLowerCase()}\\b`));
			expect(yamlB).toMatch(new RegExp(`persona: ${name}\\.`));
		}
		expect(yamlB.match(/^ {2}id: /gm)).toHaveLength(3);
		expect(yamlB).toMatch(/push: true/);
		const kindsB = events(b.project).map((e) => e.kind);
		const updated = kindsB.lastIndexOf("team.updated");
		expect(updated).toBeGreaterThan(-1);
		expect(kindsB.indexOf("team.resumed", updated)).toBeGreaterThan(updated);

		// 3. Project A again: Pair, on the live team. Theo worked, so is retired; Ada never did.
		copyFileSync(
			join(import.meta.dirname, "fixtures/pair-template.yaml"),
			join(templates, "pair.yaml"),
		);
		await page.goto(inA("/team"));
		await page.getByRole("button", { name: "Use a saved team" }).click();
		const dialog = page.getByRole("dialog");
		await dialog.getByLabel(/^Pair/).check();
		await dialog.getByRole("button", { name: "Show what changes" }).click();
		const group = (name: string) =>
			dialog.getByRole("region", { name: new RegExp(`^${name}`) });
		await expect(group("Stays")).toContainText("Mira");
		await expect(group("Joins")).toContainText("Noor");
		await expect(group("Retired")).toContainText("Theo");
		await expect(group("Removed")).toContainText("Ada");
		await shots(page, "preview");
		await dialog.getByRole("button", { name: "Use this team" }).click();
		await expect(dialog).toHaveCount(0);
		await expect
			.poll(() => statuses(a.project))
			.toEqual(["mira active", "theo retired", "noor active"]);
		expect(teamOf(a.project)).toMatch(/run_commands: false/);
		const last = events(a.project).slice(-2);
		expect(last.map((e) => e.kind)).toEqual(["agent.updated", "team.updated"]);
		expect(last[0]?.body).toMatchObject({
			agent_id: "theo",
			status: "retired",
		});
		expect(last[1]?.body).toMatchObject({ template: "Pair" });

		// 4. A refused result: Noor paused and a new Developer who never worked, so Pair would
		// leave no active Developer, and nothing changes.
		await expect(
			page.getByRole("button", { name: "Pause Noor" }),
		).toBeVisible();
		await page.getByRole("button", { name: "Add someone" }).click();
		await expect.poll(() => statuses(a.project)).toHaveLength(4);
		await page.getByRole("button", { name: "Pause Noor" }).click();
		await expect.poll(() => statuses(a.project)).toContain("noor paused");
		const unchanged = teamOf(a.project);
		await page.getByRole("button", { name: "Use a saved team" }).click();
		await dialog.getByLabel(/^Pair/).check();
		await dialog.getByRole("button", { name: "Show what changes" }).click();
		await expect(dialog.getByRole("alert")).toContainText(
			"Your team would have no active Developer. Noor stays but is paused. Resume Noor on the Team page first, or choose another saved team.",
		);
		await expect(
			dialog.getByRole("button", { name: "Use this team" }),
		).toBeDisabled();
		await shots(page, "refused");
		await dialog.getByRole("button", { name: "Back" }).click();
		await dialog.getByRole("button", { name: "Cancel" }).click();
		expect(teamOf(a.project)).toBe(unchanged);

		// 5. Settings: Three of us renamed, Pair deleted.
		await page.goto(inA("/settings"));
		const savedTeams = page.getByRole("region", { name: "Saved teams" });
		await savedTeams
			.getByRole("button", { name: "Rename Three of us" })
			.click();
		await savedTeams.getByLabel("New name for Three of us").fill("Our trio");
		await savedTeams.getByRole("button", { name: "Save name" }).click();
		await expect(
			savedTeams.getByRole("button", { name: "Rename Our trio" }),
		).toBeVisible();
		expect(existsSync(join(templates, "our-trio.yaml"))).toBe(true);
		expect(existsSync(saved)).toBe(false);
		await savedTeams.getByRole("button", { name: "Delete Pair" }).click();
		await savedTeams.getByRole("button", { name: "Delete it" }).click();
		await expect(
			savedTeams.getByRole("button", { name: "Delete Pair" }),
		).toHaveCount(0);
		expect(existsSync(join(templates, "pair.yaml"))).toBe(false);
		await expect(savedTeams).toContainText(
			`Kept on this computer, in ${templates}.`,
		);
		await shots(page, "settings");
	} finally {
		await other.close();
		await a.stop();
		await b.stop();
	}
});
