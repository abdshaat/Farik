import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	screen,
	waitFor,
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
		// The shell asks first, then the page itself.
		await answerStatus(socket, false);
		await answerStatus(socket, false, 2);
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
		await answerStatus(socket as never, false);
		act(() => {
			for (const seq of [1, 2, 3]) socket?.event(seq);
		});
		const table = await screen.findByRole("table");
		const rows = within(table).getAllByRole("row");
		expect(
			within(rows[1] as HTMLElement).getAllByRole("cell")[0]?.textContent,
		).toBe("3");
		expect(rows).toHaveLength(4);
		// The time fits a phone: hours, minutes and seconds, with the full value on hover.
		const time = within(rows[1] as HTMLElement).getAllByRole("cell")[1];
		expect(time?.textContent).toMatch(/^\d\d:\d\d:56$/);
		expect(time?.querySelector("time")?.getAttribute("title")).toBe(
			"2026-09-29T12:34:56.789012Z",
		);
		await expectNoAxeViolations(container);
	});

	it("lists_the_last_hundred_events", async () => {
		const { socket } = await renderApp("/events");
		await answerStatus(socket as never, false);
		act(() => {
			for (let seq = 1; seq <= 101; seq++) socket?.event(seq);
		});
		const rows = within(await screen.findByRole("table")).getAllByRole("row");
		// A header row and a hundred events, the newest first.
		expect(rows).toHaveLength(101);
		expect(
			within(rows[1] as HTMLElement).getAllByRole("cell")[0]?.textContent,
		).toBe("101");
	});

	it("says_there_is_no_page_here", async () => {
		const { container, socket } = await renderApp("/nowhere");
		await answerStatus(socket as never, false);
		expect(
			await screen.findByRole("heading", { name: en.noPage }),
		).toBeTruthy();
		expect(
			screen.getByRole("link", { name: en.noPageHome }).getAttribute("href"),
		).toBe("/");
		await expectNoAxeViolations(container);
	});

	it.each([
		"/requests/FRK-99",
		"/tasks/FRK-99/questions",
		"/tasks/FRK-99/plan",
		"/tasks/FRK-99/plan/edit",
		"/tasks/FRK-99/accept",
		"/tasks/FRK-99/help",
	])("says_a_task_it_cannot_read_at_%s", async (path) => {
		const { container, socket } = await renderApp(path);
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false);
		const asked = () =>
			socket.calls("query").filter((q) => q.params.name !== "serve.status");
		await waitFor(() => expect(asked().length).toBeGreaterThan(0));
		for (const q of asked())
			await socket.fail(q, -32002, "there is no task FRK-99");
		expect((await screen.findByRole("alert")).textContent).toBe(
			"There is no task FRK-99",
		);
		expect(
			screen.getByRole("link", { name: en.backToToday }).getAttribute("href"),
		).toBe("/");
		await expectNoAxeViolations(container);
	});
});
