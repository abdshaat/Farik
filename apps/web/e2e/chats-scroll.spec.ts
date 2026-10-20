import { expect, test } from "@playwright/test";
import { catervas, startServe } from "./fixtures/serve.ts";
import { narrow, wide } from "./fixtures/shots.ts";

/** What Theo's recorded reply says. */
const REPLY = "the login form is done and its tests are next.";

test("the chats open at the newest message, each pane scrolling on its own, and older ones load on scrolling up", async ({
	page,
}) => {
	// A hundred and thirty posts, each through the command line.
	test.setTimeout(90_000);
	const serve = await startServe({
		team: "pm-architect-developer",
		// Two recorded replies, each waiting three seconds before it plays: the Product Manager's,
		// to the posts that name no one (ADR 0079), and Theo's.
		transcripts: ["reply_to_a_mention", "reply_to_a_mention"],
		paceMs: 3000,
	});
	try {
		// A post that names no one goes to the Product Manager, so the team is paused while they
		// are made, and Mira's one reply does not land among them.
		catervas(serve.project, ["pause"]);
		for (let n = 1; n <= 130; n++)
			catervas(serve.project, ["say", `Line ${n}.`]);
		await page.goto(serve.url);
		await wide(page);
		await page.getByRole("link", { name: "Chats" }).first().click();
		await expect(page).toHaveURL(/\/channel$/);

		// 1. The newest first, the window not scrolling, the first page being 31 to 130.
		await expect(page.getByText("Line 130.", { exact: true })).toBeInViewport({
			ratio: 1,
		});
		await expect(page.getByLabel("Post to the team")).toBeInViewport({
			ratio: 1,
		});
		expect(
			await page.evaluate(
				() => document.documentElement.scrollHeight <= innerHeight,
			),
		).toBe(true);
		await expect(page.getByText("Line 30.", { exact: true })).toHaveCount(0);

		// 2. Scrolling to the top reads the page before, and what the user sees stays where it was.
		const team = page.getByRole("region", { name: "Team" });
		const list = page.getByRole("navigation", { name: "Chats" });
		const listTop = await list.evaluate(
			(nav) => nav.getBoundingClientRect().top,
		);
		const was = await team.evaluate((box) => {
			box.scrollTop = 0;
			const line = [...box.querySelectorAll("li")].find((li) =>
				li.textContent?.includes("Line 31."),
			);
			return line?.getBoundingClientRect().top ?? Number.NaN;
		});
		await expect(page.getByText("Line 30.", { exact: true })).toHaveCount(1);
		const now = await team.evaluate((box) => {
			const line = [...box.querySelectorAll("li")].find((li) =>
				li.textContent?.includes("Line 31."),
			);
			return line?.getBoundingClientRect().top ?? Number.NaN;
		});
		expect(Math.abs(now - was)).toBeLessThanOrEqual(4);
		expect(await list.evaluate((nav) => nav.scrollTop)).toBe(0);
		expect(await list.evaluate((nav) => nav.getBoundingClientRect().top)).toBe(
			listTop,
		);

		// 3. The user's own post is followed; an agent's reply, while reading old ones, is offered.
		const asked = "@theo can you look at the menu page?";
		catervas(serve.project, ["resume"]);
		catervas(serve.project, ["say", asked]);
		// The chats list's Team row previews it too ("You: …"); the post is the one in the chat.
		await expect(team.getByText(asked)).toBeInViewport({ ratio: 1 });
		await expect(
			page.getByRole("button", { name: "New messages" }),
		).toHaveCount(0);
		await team.evaluate((box) => {
			box.scrollTop = 0;
		});
		const reply = team.getByText(REPLY).last();
		await expect(
			page.getByRole("button", { name: "New messages" }),
		).toBeVisible({ timeout: 15_000 });
		await expect(reply).not.toBeInViewport();
		await page.getByRole("button", { name: "New messages" }).click();
		await expect(reply).toBeInViewport({ ratio: 1 });

		// 4. A phone: the newest and the box in view, and the window still.
		await narrow(page);
		await page.goto(`http://127.0.0.1:${serve.port}/channel`);
		await expect(
			page.getByRole("region", { name: "Team" }).getByText(REPLY).last(),
		).toBeInViewport({ ratio: 1 });
		await expect(page.getByLabel("Message the team")).toBeInViewport({
			ratio: 1,
		});
		expect(
			await page.evaluate(
				() => document.documentElement.scrollHeight <= innerHeight,
			),
		).toBe(true);
	} finally {
		await serve.stop();
	}
});
