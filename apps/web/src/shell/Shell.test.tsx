import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { media } from "../test/media.ts";
import { answerStatus, eventArrives, renderApp } from "../test/render-app.tsx";
import { refusedBy } from "../test/schema.ts";
import styles from "./Shell.module.css";

const WIDE = "(min-width: 1024px)";

describe("shell", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_the_rail_on_a_wide_screen_and_a_bar_on_a_phone", async () => {
		media.set(WIDE, true);
		const { container, socket } = await renderApp("/settings");
		await answerStatus(socket as never, false);
		const rail = await screen.findByRole("navigation", { name: en.navRail });
		const places = within(rail).getAllByRole("link");
		expect(places[0]?.textContent).toBe(en.today);
		expect(places.at(-1)?.textContent).toBe(en.settings);
		expect(within(rail).queryByRole("link", { name: en.events })).toBeNull();
		// The event list stays, at the bottom of Settings.
		expect(
			screen
				.getByRole("link", { name: en.eventsForTesting })
				.getAttribute("href"),
		).toBe("/events");
		expect(screen.queryByRole("navigation", { name: en.navBar })).toBeNull();
		await expectNoAxeViolations(container);

		act(() => media.set(WIDE, false));
		const bar = screen.getByRole("navigation", { name: en.navBar });
		for (const name of [en.today, en.team])
			expect(within(bar).getByRole("link", { name })).toBeTruthy();
		expect(screen.queryByRole("navigation", { name: en.navRail })).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("orders_the_rail", async () => {
		media.set(WIDE, true);
		const { container, socket } = await renderApp("/team");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false);
		const rail = await screen.findByRole("navigation", { name: en.navRail });
		const words = (nav: HTMLElement) =>
			within(nav)
				.getAllByRole("link")
				.map((a) => [a.textContent, a.getAttribute("href")]);
		expect(words(rail)).toEqual([
			[en.today, "/"],
			[en.board, "/board"],
			[en.chats, "/channel"],
			[en.team, "/team"],
			[en.costs, "/costs"],
			[en.settings, "/settings"],
		]);
		await expectNoAxeViolations(container);

		// On a phone, the bar has five places, and Settings sits under Team.
		act(() => media.set(WIDE, false));
		const bar = screen.getByRole("navigation", { name: en.navBar });
		expect(words(bar)).toEqual([
			[en.today, "/"],
			[en.board, "/board"],
			[en.chats, "/channel"],
			[en.team, "/team"],
			[en.costs, "/costs"],
		]);
		expect(
			screen.getByRole("link", { name: en.settings }).getAttribute("href"),
		).toBe("/settings");
		await expectNoAxeViolations(container);
	});

	it("pauses_and_resumes_the_team", async () => {
		media.set(WIDE, true);
		const { container, socket } = await renderApp("/events");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false);
		expect(await screen.findByText(en.teamWorking)).toBeTruthy();

		fireEvent.click(await screen.findByRole("button", { name: en.pauseTeam }));
		const sent = socket.calls("command")[0];
		if (!sent) throw new Error("no command was sent");
		expect(sent.params).toEqual({
			command: { command: "team_pause", body: {} },
		});
		const pending = screen.getByRole("button", { name: /Pause the team/ });
		expect(pending.getAttribute("aria-busy")).toBe("true");
		expect(
			screen.queryByText(/Nothing new starts until you resume/),
		).toBeNull();

		await socket.reply(sent, { said: "paused the team", events: [1] });
		await eventArrives(socket, 1); // team.paused
		await answerStatus(socket, true, 2);
		expect(
			await screen.findByRole("button", { name: en.resumeTeam }),
		).toBeTruthy();
		expect(screen.getByText(en.teamPaused)).toBeTruthy();
		expect(
			screen.getByText(/Nothing new starts until you resume/),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("button", { name: en.resumeTeam }));
		await waitFor(() =>
			expect(socket.calls("command")[1]?.params).toEqual({
				command: { command: "team_resume", body: {} },
			}),
		);
	});

	it("shows_a_refusal_in_words", async () => {
		media.set(WIDE, false);
		const { container, socket } = await renderApp("/events");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, true);
		fireEvent.click(await screen.findByRole("button", { name: en.resume }));
		const sent = socket.calls("command")[0];
		if (!sent) throw new Error("no command was sent");
		await socket.reply(sent, {
			error: {
				kind: "refused",
				detail: "not_paused: the team is not paused",
			},
		});
		expect(await screen.findByText(en.refuseNotPaused)).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("breathes_the_dot_while_connected", async () => {
		media.set(WIDE, true);
		const { socket } = await renderApp("/team");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false);
		const dot = (await screen.findByText(en.connected)).querySelector("span");
		expect(dot?.classList).toContain(styles.dot);
		expect(dot?.classList).toContain(styles.live);
	});

	it("keeps_the_connected_dot_still_under_reduced_motion", () => {
		const css = readFileSync(
			join(import.meta.dirname, "Shell.module.css"),
			"utf8",
		).replace(/\s+/g, " ");
		expect(css).toMatch(
			/\.live \{[^}]*animation: breathe 3s ease-in-out infinite;/,
		);
		expect(css).toMatch(
			/@media \(prefers-reduced-motion: reduce\) \{ \.live \{ animation: none; \} \}/,
		);
	});

	it("names_the_project_folder_above_connected", async () => {
		media.set(WIDE, true);
		const { socket } = await renderApp("/team");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false, 1, { project_root: "/h/work/old-repo" });
		const name = await screen.findByText("old-repo");
		expect(name.getAttribute("title")).toBe("/h/work/old-repo");
		const connected = screen.getByText(en.connected);
		expect(
			name.compareDocumentPosition(connected) &
				Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
	});

	it("offers_other_keys_until_one_is_chosen", async () => {
		media.set(WIDE, true);
		const { socket } = await renderApp("/board");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false, 1, {
			keys_copied: { from: "/h/old-repo", count: 2 },
		});
		const notice = await screen.findByRole("status");
		expect(notice.textContent).toContain("old-repo");
		expect(notice.textContent).toContain("2 services");
		fireEvent.click(screen.getByRole("button", { name: en.keysKeep }));
		const sent = await waitFor(() => {
			const f = socket.calls("keys_copied.dismiss").at(-1);
			if (!f) throw new Error("not sent");
			return f;
		});
		expect(sent.params).toEqual({});
		expect(refusedBy("keysCopiedDismissRequest", sent.params)).toEqual([]);
		await socket.reply(sent, {});
		await answerStatus(socket, false, 2, { keys_copied: null });
		await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
	});

	it("says_one_service_in_the_singular", async () => {
		media.set(WIDE, true);
		const { socket } = await renderApp("/board");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false, 1, {
			keys_copied: { from: "/h/old-repo", count: 1 },
		});
		expect((await screen.findByRole("status")).textContent).toContain(
			"(1 service)",
		);
	});

	it("choosing_different_keys_goes_to_the_team_page", async () => {
		media.set(WIDE, true);
		const { socket } = await renderApp("/board");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false, 1, {
			keys_copied: { from: "/h/old-repo", count: 2 },
		});
		fireEvent.click(await screen.findByRole("button", { name: en.keysChoose }));
		const sent = await waitFor(() => {
			const f = socket.calls("keys_copied.dismiss").at(-1);
			if (!f) throw new Error("not sent");
			return f;
		});
		expect(sent.params).toEqual({});
		await socket.reply(sent, {});
		await answerStatus(socket, false, 2, { keys_copied: null });
		await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
		const rail = screen.getByRole("navigation", { name: en.navRail });
		await waitFor(() =>
			expect(
				within(rail)
					.getByRole("link", { name: en.team })
					.getAttribute("aria-current"),
			).toBe("page"),
		);
	});
});
