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
		expect(await screen.findByText("The team is not paused")).toBeTruthy();
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
});
