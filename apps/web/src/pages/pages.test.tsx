import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	screen,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { answerStatus, renderApp } from "../test/render-app.tsx";

describe("pages", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		localStorage.clear();
	});

	it("settings_changes_the_theme_and_disconnects", async () => {
		const { container, fetch, socket } = await renderApp("/settings", {
			"GET /session": 204,
			"POST /disconnect": 204,
		});
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false);
		expect(await screen.findByText("/home/me/corner-bakery")).toBeTruthy();
		expect(screen.getByText("127.0.0.1:7420")).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("radio", { name: /^Dark/ }));
		expect(document.documentElement.dataset.theme).toBe("dark");

		const advanced = screen.getByRole("switch", { name: en.advancedSwitch });
		fireEvent.click(advanced);
		expect(localStorage.getItem("farik.advanced")).toBe("true");
		expect(advanced.getAttribute("aria-checked")).toBe("true");
		fireEvent.click(advanced);
		expect(localStorage.getItem("farik.advanced")).toBe("false");

		fireEvent.click(screen.getByRole("button", { name: en.disconnect }));
		expect(
			await screen.findByRole("heading", { name: en.noSessionTitle }),
		).toBeTruthy();
		expect(fetch).toHaveBeenCalledWith(
			"/disconnect",
			expect.objectContaining({ method: "POST", credentials: "same-origin" }),
		);
	});

	it("shows_the_connect_page_states", async () => {
		const first = await renderApp("/events", { "GET /session": 401 });
		expect(
			await screen.findByRole("heading", { name: en.noSessionTitle }),
		).toBeTruthy();
		const code = first.container.querySelector("code");
		expect(code?.textContent).toBe("farik serve");
		expect(screen.getByRole("button", { name: en.copy })).toBeTruthy();
		// The mark is decoration beside the wordmark, which names Farik.
		expect(screen.getByRole("img", { name: en.brand })).toBeTruthy();
		expect(first.container.querySelectorAll('img[alt=""]').length).toBe(1);
		await expectNoAxeViolations(first.container);
		cleanup();

		const { container, socket } = await renderApp("/events");
		act(() => socket?.close());
		expect(
			await screen.findByRole("heading", { name: en.lostTitle }),
		).toBeTruthy();
		expect(screen.getByText(new RegExp(en.lostRetry))).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("lists_the_newest_events_first", async () => {
		const { container, socket } = await renderApp("/events");
		act(() => {
			for (const seq of [1, 2, 3]) socket?.event(seq);
		});
		const table = await screen.findByRole("table");
		const rows = within(table).getAllByRole("row");
		expect(
			within(rows[1] as HTMLElement).getAllByRole("cell")[0]?.textContent,
		).toBe("3");
		expect(rows).toHaveLength(4);
		await expectNoAxeViolations(container);
	});

	it("says_there_is_no_page_here", async () => {
		const { container } = await renderApp("/nowhere");
		expect(screen.getByRole("heading", { name: en.noPage })).toBeTruthy();
		expect(
			screen.getByRole("link", { name: en.noPageHome }).getAttribute("href"),
		).toBe("/");
		await expectNoAxeViolations(container);
	});
});
