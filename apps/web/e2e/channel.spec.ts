import { expect, type Locator, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

/** The contrast ratio of an element's text colour on its own background colour. */
function contrast(element: Locator): Promise<number> {
	return element.evaluate((node) => {
		const style = getComputedStyle(node);
		const luminance = (rgb: string) => {
			const [r, g, b] = (rgb.match(/[\d.]+/g) ?? []).map((c) => {
				const s = Number(c) / 255;
				return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
			});
			return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);
		};
		const [light, dark] = [
			luminance(style.color),
			luminance(style.backgroundColor),
		].sort((a, b) => b - a);
		return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05);
	});
}

/**
 * A "(waiting on you)" note in your own bubble, as a task waiting on you puts it there: the
 * contrast of its words on the bubble, in the light theme and then the dark.
 */
function waitingNoteContrast(bubble: Locator): Promise<number[]> {
	return bubble.evaluate((text) => {
		const rules = [...document.styleSheets].flatMap((sheet) => {
			try {
				return [...sheet.cssRules];
			} catch {
				return []; // another origin's sheet
			}
		});
		// MessageText's note: the rule named for waiting that mutes its words.
		const rule = rules.find(
			(r): r is CSSStyleRule =>
				r instanceof CSSStyleRule &&
				/^\.[\w-]*waiting[\w-]*$/.test(r.selectorText) &&
				r.style.color === "var(--farik-color-ink-muted)",
		);
		if (!rule) throw new Error("no rule for the waiting note");
		const note = document.createElement("span");
		note.className = rule.selectorText.slice(1);
		note.textContent = " (waiting on you)";
		text.append(note);
		const luminance = (rgb: string) => {
			const [r, g, b] = (rgb.match(/[\d.]+/g) ?? []).map((c) => {
				const s = Number(c) / 255;
				return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
			});
			return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);
		};
		const root = document.documentElement;
		const was = root.dataset.theme;
		const ratios = ["light", "dark"].map((theme) => {
			root.dataset.theme = theme;
			const [light, dark] = [
				luminance(getComputedStyle(note).color),
				luminance(getComputedStyle(text).backgroundColor),
			].sort((a, b) => b - a);
			return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05);
		});
		if (was === undefined) delete root.dataset.theme;
		else root.dataset.theme = was;
		note.remove();
		return ratios;
	});
}

test("the user mentions an agent in the channel, and the agent's reply appears by itself", async ({
	page,
}) => {
	const serve = await startServe({
		team: "pm-architect-developer",
		transcripts: ["reply_to_a_mention"],
	});
	try {
		await page.goto(serve.url);
		await page.getByRole("link", { name: "Channel" }).first().click();
		await expect(page).toHaveURL(/\/channel$/);

		// 1. The post, with its mention, picked from the list with Enter, which types no new line.
		const box = page.getByLabel("Post to the team");
		await box.pressSequentially("@th");
		await expect(page.getByRole("option", { name: "Theo" })).toBeVisible();
		await box.press("Enter");
		await expect(box).toHaveValue("@theo ");
		await box.fill("@theo can you look at the menu page?");
		await page.getByRole("button", { name: "Post" }).click();

		// 2. The post, labelled You, with the mention shown by name.
		const messages = page.getByRole("list", { name: "Messages" });
		const mine = messages
			.getByRole("listitem")
			.filter({ hasText: "can you look at the menu page?" });
		await expect(mine).toContainText("You");
		await expect(mine.locator("mark")).toHaveText("@Theo");
		// The mention sets its own background, so its words must stand out on it (WCAG AA).
		expect(await contrast(mine.locator("mark"))).toBeGreaterThanOrEqual(4.5);
		// So must a note that a task waits on you, in both themes.
		for (const ratio of await waitingNoteContrast(
			mine.locator("p").filter({ has: page.locator("mark") }),
		))
			expect(ratio).toBeGreaterThanOrEqual(4.5);
		await expect(page.getByLabel("Post to the team")).toHaveValue("");
		expect(
			events(serve.project).find((e) => e.kind === "message.posted")?.body,
		).toMatchObject({
			author: "human",
			text: "@theo can you look at the menu page?",
			mentions: ["theo"],
		});

		// 3. Theo's reply, which appears without a reload.
		const reply = messages
			.getByRole("listitem")
			.filter({ hasText: "the login form is done and its tests are next." });
		await expect(reply).toBeVisible({ timeout: 15_000 });
		await expect(reply).toContainText("Theo");
		await expect(reply.getByText("Replying to You")).toBeVisible();
		await expect(reply.getByRole("link", { name: "FRK-1" })).toHaveAttribute(
			"href",
			"/tasks/FRK-1",
		);
		await screenshots(page, "channel");

		// 4. Today's preview shows both.
		await page.getByRole("link", { name: "Today" }).first().click();
		const preview = page.getByRole("region", { name: "In the channel" });
		await expect(preview).toContainText("can you look at the menu page?");
		await expect(preview).toContainText(
			"the login form is done and its tests are next.",
		);
		await expect(
			preview.getByRole("link", { name: "Open the channel" }),
		).toBeVisible();
	} finally {
		await serve.stop();
	}
});
