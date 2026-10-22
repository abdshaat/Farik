import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, renderApp, teamList } from "../test/render-app.tsx";

/** A working team with a thing waiting, a paused one, and one whose folder is gone. */
function listing(extra: Record<string, unknown> = {}) {
	const [shop, bakery, garden] = teamList("shop", "bakery", "garden").teams;
	return {
		...teamList("shop", "bakery", "garden"),
		teams: [
			{
				...shop,
				name: "Corner Shop",
				working_agents: 2,
				today_usd: 4.1,
				month_usd: 61.8,
				waiting: 1,
			},
			{ ...bakery, name: "Bakery", status: "paused" },
			{ ...garden, name: "Garden", status: "missing" },
		],
		...extra,
	};
}

const card = (name: string) =>
	screen
		.getByRole("heading", { name, level: 2 })
		.closest("article") as HTMLElement;

describe("all teams", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
	});

	it("shows_a_card_per_team", async () => {
		const { hub, sockets } = await renderApp("/teams");
		const h = hub as FakeSocket;
		await answerQuery(h, "teams.list", listing());
		await screen.findByRole("heading", { name: en.allTeams, level: 1 });

		const shop = within(card("Corner Shop"));
		expect(shop.getByText("1 waiting on you")).toBeTruthy();
		expect(shop.getByText("2 agents working")).toBeTruthy();
		expect(shop.getByText("$4.10 today")).toBeTruthy();
		expect(shop.getByText("$61.80 this month")).toBeTruthy();
		expect(
			shop.getByRole("link", { name: "Corner Shop" }).getAttribute("href"),
		).toBe("/t/shop/");

		// A paused team starts on a socket of its own, which closes after.
		const bakery = within(card("Bakery"));
		expect(bakery.getByText("Paused")).toBeTruthy();
		const lists = () =>
			h.calls("query").filter((f) => f.params.name === "teams.list").length;
		const before = lists();
		fireEvent.click(bakery.getByRole("button", { name: en.teamStart }));
		const team = await waitFor(() => {
			const s = sockets.find((one) => one.url.endsWith("/t/bakery/rpc"));
			if (!s) throw new Error("no socket to the paused team");
			return s;
		});
		const resume = await waitFor(() => {
			const frame = team.calls("command")[0];
			if (!frame) throw new Error("no command was sent");
			return frame;
		});
		expect(resume.params).toEqual({
			command: { command: "team_resume", body: {} },
		});
		await team.reply(resume, { said: "resumed", events: [4] });
		await waitFor(() => expect(team.closed).toBe(true));
		// The list is asked again.
		await waitFor(() => expect(lists()).toBeGreaterThan(before));

		// A team whose project is gone is removed after asking; no file is touched.
		const garden = within(card("Garden"));
		expect(garden.getByText(en.teamsBadgeMissing)).toBeTruthy();
		fireEvent.click(garden.getByRole("button", { name: en.teamRemove }));
		expect(
			screen.getByText(
				"Remove Garden from Catervas? Its folder isn't touched.",
			),
		).toBeTruthy();
		expect(h.calls("teams.remove")).toHaveLength(0);
		fireEvent.click(
			within(card("Garden")).getByRole("button", { name: en.teamRemoveYes }),
		);
		const removal = await waitFor(() => {
			const frame = h.calls("teams.remove")[0];
			if (!frame) throw new Error("no teams.remove was sent");
			return frame;
		});
		expect(removal.params).toEqual({ slug: "garden", confirmed: true });

		// What waits on the owner, by team.
		const waiting = screen.getByRole("region", { name: en.teamsWaitingTitle });
		expect(
			within(waiting)
				.getByRole("link", { name: "Corner Shop: 1 thing" })
				.getAttribute("href"),
		).toBe("/t/shop/");
		expect(within(waiting).queryByText(/Bakery/)).toBeNull();
		expect(screen.queryByText(/limit is reached/)).toBeNull();
	});

	it("says_when_the_account_sleeps", async () => {
		const until = "2026-10-22T15:40:00Z";
		const { hub } = await renderApp("/teams");
		await answerQuery(hub as FakeSocket, "teams.list", {
			...listing(),
			account_asleep_until: until,
		});
		const at = new Date(until)
			.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" })
			.toLowerCase();
		expect(
			await screen.findByText(
				`Your AI account's limit is reached. Your teams go on at ${at}.`,
			),
		).toBeTruthy();
	});
});
