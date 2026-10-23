import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { media } from "../test/media.ts";
import {
	answerQuery,
	answerStatus,
	renderApp,
	teamList,
	where,
} from "../test/render-app.tsx";

const WIDE = "(min-width: 1024px)";

/** Three teams: shop (the one open), bakery with one thing waiting, and a paused one. */
function three() {
	const listed = teamList("shop", "bakery", "garden");
	const [shop, bakery, garden] = listed.teams;
	return {
		...listed,
		teams: [
			{ ...shop, name: "Corner Shop", working_agents: 2 },
			{ ...bakery, name: "Bakery", waiting: 1 },
			{ ...garden, name: "Garden", status: "paused" },
		],
	};
}

describe("team switcher", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		media.set(WIDE, true);
	});

	it("switches_team_from_the_rail", async () => {
		media.set(WIDE, true);
		const { socket, hub } = await renderApp("/t/shop/tasks/CTV-3");
		await answerStatus(socket as FakeSocket, false);
		await answerQuery(hub as FakeSocket, "teams.list", three());
		const button = await screen.findByRole("button", { name: "Corner Shop" });

		fireEvent.click(button);
		const menu = await screen.findByRole("menu", { name: "Teams" });
		const items = within(menu).getAllByRole("menuitem");
		expect(items.map((item) => item.textContent)).toEqual([
			en.allTeams,
			"Corner Shopworking",
			"Bakery1 waiting",
			"Gardenpaused",
			en.teamsAdd,
		]);

		fireEvent.click(within(menu).getByRole("menuitem", { name: /^Bakery/ }));
		const chosen = await waitFor(() => {
			const frame = (hub as FakeSocket).calls("teams.current")[0];
			if (!frame) throw new Error("no teams.current was sent");
			return frame;
		});
		expect(chosen.params).toEqual({ slug: "bakery" });
		await (hub as FakeSocket).reply(chosen, {});
		// A task's page is the old team's: the new team opens at Today.
		await waitFor(() => expect(where()).toBe("/t/bakery/"));
	});

	it("keeps_the_page_and_closes_on_escape", async () => {
		media.set(WIDE, true);
		const { socket, hub, sockets } = await renderApp("/t/shop/board");
		await answerStatus(socket as FakeSocket, false);
		await answerQuery(hub as FakeSocket, "teams.list", three());
		fireEvent.click(await screen.findByRole("button", { name: "Corner Shop" }));
		await screen.findByRole("menu", { name: "Teams" });
		fireEvent.keyDown(document, { key: "Escape" });
		expect(screen.queryByRole("menu")).toBeNull();

		fireEvent.click(screen.getByRole("button", { name: "Corner Shop" }));
		fireEvent.click(
			within(await screen.findByRole("menu")).getByRole("menuitem", {
				name: /^Garden/,
			}),
		);
		const chosen = await waitFor(() => {
			const frame = (hub as FakeSocket).calls("teams.current")[0];
			if (!frame) throw new Error("no teams.current was sent");
			return frame;
		});
		await (hub as FakeSocket).reply(chosen, {});
		await waitFor(() => expect(where()).toBe("/t/garden/board"));
		expect(sockets.some((s) => s.url.endsWith("/t/garden/rpc"))).toBe(true);
	});
});
