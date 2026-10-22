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
import { answerCalls, IDLE, NOTHING, row } from "../test/finding.ts";
import {
	answerQuery,
	renderApp,
	teamList,
	where,
} from "../test/render-app.tsx";

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

	it("offers_undo_until_a_team_starts", async () => {
		const { hub, sockets } = await renderApp("/teams");
		const h = hub as FakeSocket;
		const table: Record<string, unknown> = {
			"finding.status": IDLE,
			"finding.list": {
				...NOTHING,
				main: [
					row({
						key: "/home/me/recipe-cards",
						name: "recipe-cards",
						suggested: "watch",
					}),
				],
			},
			"finding.choose": {},
			"teams.undo": { removed: true },
		};
		answerCalls(h, table);
		const [made, started] = teamList("fresh", "going").teams;
		await answerQuery(h, "teams.list", {
			...teamList("fresh", "going"),
			teams: [
				{ ...made, name: "Fresh", status: "paused", undoable: true },
				{ ...started, name: "Going", status: "paused", undoable: false },
			],
		});

		// A team made from the proposal and not started: Start, Document it first and Undo.
		const fresh = within(card("Fresh"));
		expect(fresh.getByRole("button", { name: "Start" })).toBeTruthy();
		expect(
			fresh.getByRole("button", { name: "Document it first" }),
		).toBeTruthy();
		expect(fresh.getByRole("button", { name: "Undo" })).toBeTruthy();
		// One that has started is only paused.
		const going = within(card("Going"));
		expect(going.queryByRole("button", { name: "Undo" })).toBeNull();
		expect(
			going.queryByRole("button", { name: "Document it first" }),
		).toBeNull();

		// Document it first is the documenting offer, on the team's own socket.
		fireEvent.click(fresh.getByRole("button", { name: "Document it first" }));
		const team = await waitFor(() => {
			const s = sockets.find((one) => one.url.endsWith("/t/fresh/rpc"));
			if (!s) throw new Error("no socket to the team");
			return s;
		});
		const documenting = await waitFor(() => {
			const frame = team.calls("command")[0];
			if (!frame) throw new Error("no command was sent");
			return frame;
		});
		expect(documenting.params).toEqual({
			command: { command: "catervafication_start", body: {} },
		});
		await team.reply(documenting, { said: "documenting", events: [5] });

		// Undo asks first, then removes.
		fireEvent.click(fresh.getByRole("button", { name: "Undo" }));
		expect(h.calls("teams.undo")).toHaveLength(0);
		fireEvent.click(
			within(card("Fresh")).getByRole("button", { name: "Yes, undo" }),
		);
		await waitFor(() =>
			expect(h.calls("teams.undo").map((f) => f.params)).toEqual([
				{ slug: "fresh" },
			]),
		);

		// The projects watched, not given a team: Make a team marks Team and goes on to the proposal.
		const watched = within(screen.getByRole("region", { name: "Watched" }));
		expect(watched.getByText("recipe-cards")).toBeTruthy();
		fireEvent.click(watched.getByRole("button", { name: "Make a team" }));
		await waitFor(() =>
			expect(h.calls("finding.choose").map((f) => f.params)).toEqual([
				{ key: "/home/me/recipe-cards", choice: "team" },
			]),
		);
		await waitFor(() => expect(where()).toBe("/find/teams"));
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
	it("offers_to_find_other_projects_on_all_teams", async () => {
		const { hub } = await renderApp("/teams");
		const h = hub as FakeSocket;
		const table: Record<string, unknown> = {
			"finding.status": IDLE,
			"finding.start": { started: true },
			"finding.list": NOTHING,
		};
		answerCalls(h, table);
		await answerQuery(h, "teams.list", listing());

		// With nothing found yet, the page offers it, above the cards.
		expect(await screen.findByText("Find your other projects")).toBeTruthy();
		expect(screen.getByText("You choose which get a team.")).toBeTruthy();
		fireEvent.click(
			screen.getByRole("button", { name: "Look for my projects" }),
		);
		await waitFor(() => expect(h.calls("finding.start")).toHaveLength(1));
		await waitFor(() => expect(where()).toBe("/find/scanning"));
	});

	it("keeps_the_offer_away_once_declined_and_says_what_a_scan_left", async () => {
		const { hub } = await renderApp("/teams");
		const h = hub as FakeSocket;
		const table: Record<string, unknown> = {
			"finding.status": IDLE,
			"finding.start": { started: true },
			"finding.list": NOTHING,
		};
		answerCalls(h, table);
		await answerQuery(h, "teams.list", listing());
		fireEvent.click(await screen.findByRole("button", { name: "Not now" }));
		await waitFor(() =>
			expect(screen.queryByText("Find your other projects")).toBeNull(),
		);
		expect(localStorage.getItem("catervas.findNotNow")).not.toBeNull();

		// A scan that stopped halfway can go on.
		table["finding.status"] = {
			...IDLE,
			phase: "stopped",
			index_exists: true,
			projects: 5,
		};
		cleanup();
		const again = await renderApp("/teams");
		const g = again.hub as FakeSocket;
		answerCalls(g, table);
		await answerQuery(g, "teams.list", listing());
		expect(await screen.findByText("We stopped looking halfway")).toBeTruthy();
		expect(
			screen.getByText("5 projects found so far. Nothing is lost."),
		).toBeTruthy();
		expect(
			screen
				.getByRole("link", { name: "See what's found" })
				.getAttribute("href"),
		).toBe("/find/found");
		fireEvent.click(screen.getByRole("button", { name: "Keep looking" }));
		await waitFor(() => expect(g.calls("finding.start")).toHaveLength(1));

		// After a scan, the projects found since the owner last looked are one line.
		cleanup();
		table["finding.status"] = { ...IDLE, phase: "done", index_exists: true };
		table["finding.list"] = {
			...NOTHING,
			main: [
				row({ key: "/a", name: "a", last_at: new Date().toISOString() }),
				row({ key: "/b", name: "b", last_at: new Date().toISOString() }),
				row({
					key: "/c",
					name: "c",
					already_team: "shop",
					last_at: new Date().toISOString(),
				}),
			],
		};
		const done = await renderApp("/teams");
		const f = done.hub as FakeSocket;
		answerCalls(f, table);
		await answerQuery(f, "teams.list", listing());
		expect(await screen.findByText("2 new projects found")).toBeTruthy();
		expect(
			screen.getByRole("link", { name: "See them" }).getAttribute("href"),
		).toBe("/find/found");
	});
});
