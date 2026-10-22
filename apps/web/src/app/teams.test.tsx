import { cleanup, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { FakeSocket } from "../test/fake-socket.ts";
import {
	answerQuery,
	answerStatus,
	renderApp,
	teamList,
	where,
} from "../test/render-app.tsx";
import { ON_CURRENT } from "./teams.ts";

const noProject = {
	project_root: null,
	paused: false,
	signed_in: true,
	port: 7420,
	take_on_error: null,
};

describe("several teams in the address", () => {
	beforeEach(() => localStorage.clear());
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
		localStorage.clear();
	});

	it("opens_the_team_in_the_address", async () => {
		const { sockets, socket, hub, go } = await renderApp("/t/shop/board");
		const urls = () => sockets.map((s) => new URL(s.url).pathname);
		expect(urls()).toContain("/hub/rpc");
		expect(urls()).toContain("/t/shop/rpc");
		const team = socket as FakeSocket;
		const asked = (s: FakeSocket) =>
			s.calls("query").map((frame) => frame.params.name);
		await answerStatus(team, false);
		await waitFor(() => expect(asked(team)).toContain("tasks.list"));
		expect(asked(hub as FakeSocket)).not.toContain("tasks.list");

		await go("/t/bakery/");
		await waitFor(() => expect(urls()).toContain("/t/bakery/rpc"));
		expect(team.closed).toBe(true);
		const bakery = sockets.find((s) => s.url.endsWith("/t/bakery/rpc"));
		expect(bakery?.closed).toBe(false);
		expect((hub as FakeSocket).closed).toBe(false);
	});

	it("sends_an_old_address_to_the_last_team", async () => {
		const goes = async (path: string, teams: string[], last?: string) => {
			cleanup();
			localStorage.clear();
			if (last) localStorage.setItem("catervas.lastTeam", last);
			const { hub, open } = await renderApp(path, undefined, undefined, {
				raw: true,
			});
			open();
			await answerQuery(hub as FakeSocket, "teams.list", teamList(...teams));
			await answerQuery(hub as FakeSocket, "serve.status", noProject);
			await waitFor(() => expect(where()).not.toBe(path));
			return where();
		};
		expect(await goes("/board", ["bakery", "shop"], "shop")).toBe(
			"/t/shop/board",
		);
		expect(await goes("/", ["bakery", "shop"])).toBe("/teams");
		expect(await goes("/", ["bakery"])).toBe("/t/bakery/");
		expect(await goes("/", [])).toBe("/setup/computer");
	});

	it("waits_for_the_team_that_was_just_taken_on", async () => {
		const { hub, go } = await renderApp("/setup/project");
		const h = hub as FakeSocket;
		await go("/", ON_CURRENT);
		await answerQuery(h, "teams.list", teamList());
		await answerStatus(h, false, 1, noProject);
		// No team yet: its drive starts after the hub is back, so the page asks again.
		expect(where()).toBe("/");
		const lists = () =>
			h.calls("query").filter((f) => f.params.name === "teams.list").length;
		await waitFor(() => expect(lists()).toBeGreaterThan(1), { timeout: 3000 });
		expect(where()).toBe("/");
		await answerQuery(h, "teams.list", teamList("bakery"));
		await waitFor(() => expect(where()).toBe("/t/bakery/"), { timeout: 3000 });
	});
});
