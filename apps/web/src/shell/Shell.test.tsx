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
import { answerStatus, renderApp } from "../test/render-app.tsx";

const WIDE = "(min-width: 1024px)";

describe("shell", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_the_rail_on_a_wide_screen_and_a_bar_on_a_phone", async () => {
		media.set(WIDE, true);
		const { container } = await renderApp("/events");
		const rail = screen.getByRole("navigation", { name: en.navRail });
		for (const name of [en.events, en.settings])
			expect(within(rail).getByRole("link", { name })).toBeTruthy();
		expect(screen.queryByRole("navigation", { name: en.navBar })).toBeNull();
		await expectNoAxeViolations(container);

		act(() => media.set(WIDE, false));
		const bar = screen.getByRole("navigation", { name: en.navBar });
		for (const name of [en.events, en.settings])
			expect(within(bar).getByRole("link", { name })).toBeTruthy();
		expect(screen.queryByRole("navigation", { name: en.navRail })).toBeNull();
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

		act(() => socket.reply(sent, { said: "paused the team", events: [1] }));
		act(() => socket.event(1)); // team.paused
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
		act(() =>
			socket.reply(sent, {
				error: {
					kind: "refused",
					detail: "not_paused: the team is not paused",
				},
			}),
		);
		expect(await screen.findByText("The team is not paused")).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
