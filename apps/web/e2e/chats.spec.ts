import { expect, type Page, test } from "@playwright/test";
import { events, startServe } from "./fixtures/serve.ts";
import { screenshots } from "./fixtures/shots.ts";

/** The chats' screenshots are taken at 1440, where the list, the chat and the channel's meetings fit. */
const shots = (page: Page, name: string) => screenshots(page, name, 1440);

/** Asks `name` `text` in their one-to-one, with Enter. */
async function ask(page: Page, name: string, text: string) {
	const box = page.getByLabel(`Message ${name}`);
	await box.fill(text);
	await box.press("Enter");
	await expect(box).toHaveValue("");
}

test("the user chats with an agent, and sends her proposal as a request, through the real server and browser", async ({
	page,
}) => {
	const serve = await startServe({
		team: "pm-architect-developer",
		transcripts: [
			"chat_answers_with_a_request",
			"triage_frk_1_small_by_pm",
			"ask_with_choices_frk_1",
			// Ada's answer, given while she is paused.
			"chat_answers_with_a_request",
		],
	});
	const at = (path: string) => `http://127.0.0.1:${serve.port}${path}`;
	try {
		await page.goto(serve.url);

		// 1. The chat list, Team first, then Mira's chat.
		await page.getByRole("link", { name: "Chats" }).first().click();
		await expect(page).toHaveURL(/\/channel$/);
		await expect(
			page.getByText("No messages yet. Ask Mira anything."),
		).toBeVisible();
		await shots(page, "chats");
		await page.getByRole("link", { name: /Mira/ }).first().click();
		await expect(page).toHaveURL(/\/channel\/mira$/);
		await expect(
			page.getByRole("heading", { name: "Talking with Mira" }),
		).toBeVisible();

		// 2. The question, labelled You.
		await ask(page, "Mira", "Could customers also pay with Apple Pay?");
		const messages = page.getByRole("list", { name: "Messages" });
		await expect(
			messages
				.getByRole("listitem")
				.filter({ hasText: "Could customers also pay with Apple Pay?" }),
		).toContainText("You");

		// 3. Mira's reply appears by itself, with the request she proposes, and nothing reached the
		// channel.
		const reply = messages
			.getByRole("listitem")
			.filter({ hasText: "Not yet: checkout takes cards alone." });
		await expect(reply).toBeVisible({ timeout: 15_000 });
		await expect(reply).toContainText("Mira");
		const proposal = reply.getByLabel("A request Mira suggests.", {
			exact: false,
		});
		await expect(proposal).toHaveValue(
			/^Let customers pay with Apple Pay\n\nOffer Apple Pay at checkout/,
		);
		const kinds = events(serve.project).map((e) => e.kind);
		expect(kinds).toContain("chat_message.posted");
		expect(kinds).not.toContain("message.posted");
		expect(kinds).not.toContain("task.created");
		await shots(page, "chats-reply");

		// 4. The proposal, sent by the user's hand.
		await reply.getByRole("button", { name: "Send as a request" }).click();
		const sent = reply.getByRole("link", { name: "Sent as FRK-1" });
		await expect(sent).toHaveAttribute("href", "/requests/FRK-1");
		const created = events(serve.project).find(
			(e) => e.kind === "task.created",
		);
		expect(created?.task_id).toBe("FRK-1");
		expect(created?.body).toMatchObject({ created_by: "human" });
		expect(created?.body.from_chat_message).toEqual(expect.any(Number));
		await shots(page, "chats-sent");

		// 5. FRK-1 on Today, where Mira asks about it.
		await page.getByRole("link", { name: "Today" }).first().click();
		await expect(page.getByText("Mira has a question")).toBeVisible({
			timeout: 30_000,
		});
		await expect(page.getByRole("link", { name: "Answer" })).toHaveAttribute(
			"href",
			"/tasks/FRK-1/questions",
		);

		// 6. Ada, paused, still answers her chat. (The only Product Manager and Developer cannot be paused.)
		await page.getByRole("link", { name: "Team" }).first().click();
		await page.getByRole("button", { name: "Pause Ada" }).click();
		await expect(
			page.getByRole("button", { name: "Resume Ada" }),
		).toBeVisible();
		await page.goto(at("/channel/ada"));
		await ask(page, "Ada", "Can checkout take Apple Pay today?");
		await expect(
			messages
				.getByRole("listitem")
				.filter({ hasText: "Not yet: checkout takes cards alone." }),
		).toContainText("Ada", { timeout: 15_000 });
		const log = events(serve.project);
		const paused = log.findLastIndex(
			(e) =>
				e.kind === "agent.updated" &&
				e.body.agent_id === "ada" &&
				e.body.status === "paused",
		);
		const answering = log.findIndex(
			(e) =>
				e.kind === "session.started" &&
				e.body.purpose === "chat" &&
				e.agent_id === "ada",
		);
		expect(paused).toBeGreaterThan(-1);
		expect(answering).toBeGreaterThan(paused);
		await shots(page, "chats-paused");

		// 7. The chats' cost, on the Costs page.
		await page.goto(at("/costs"));
		// The two chats' cost, and none of the triage's or the refining's.
		const chatCost = events(serve.project)
			.filter((e) => e.kind === "cost.recorded" && e.body.purpose === "chat")
			.reduce((sum, e) => sum + Number(e.body.cost_usd), 0);
		expect(chatCost).toBeGreaterThan(0);
		await expect(
			page.getByText("Conversations today:").locator(".."),
		).toContainText(`Conversations today: $${chatCost.toFixed(2)}.`);
		await shots(page, "chats-costs");
	} finally {
		await serve.stop();
	}
});
