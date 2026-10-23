import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useConnection } from "../app/connection.tsx";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
import { refusedBy } from "../test/schema.ts";

function Status() {
	return <p data-testid="status">{useConnection().status}</p>;
}

describe("pages", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		localStorage.clear();
	});

	it("settings_changes_the_theme", async () => {
		const { container, socket } = await renderApp("/settings/general");
		if (!socket) throw new Error("no socket");
		// The shell asks first, and the page asks nothing.
		await answerStatus(socket, false);
		await expectNoAxeViolations(container);

		fireEvent.click(await screen.findByRole("radio", { name: /^Dark/ }));
		expect(document.documentElement.dataset.theme).toBe("dark");

		expect(
			screen.queryByRole("switch", { name: en.advancedSwitch }),
		).toBeNull();
	});

	it("settings_shows_the_computer_and_disconnects", async () => {
		const { container, fetch, socket } = await renderApp("/settings/computer", {
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

		fireEvent.click(screen.getByRole("button", { name: en.disconnect }));
		expect(
			await screen.findByRole("heading", { name: en.noSessionTitle }),
		).toBeTruthy();
		expect(fetch).toHaveBeenCalledWith(
			"/disconnect",
			expect.objectContaining({ method: "POST", credentials: "same-origin" }),
		);
	});

	it("settings_changes_the_project_after_asking", async () => {
		const { socket } = await renderApp(
			"/settings/computer",
			{ "GET /session": 204 },
			<Status />,
		);
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerStatus(s, false, 2);
		fireEvent.click(
			await screen.findByRole("button", { name: en.changeProject }),
		);
		expect(
			screen.getByText(
				en.changeProjectConfirm.replace("{name}", "corner-bakery"),
			),
		).toBeTruthy();
		// Cancel sends nothing.
		fireEvent.click(screen.getByRole("button", { name: en.agentCancel }));
		expect(screen.queryByText(/The team stops working/)).toBeNull();
		expect(s.calls("project.leave")).toHaveLength(0);

		fireEvent.click(screen.getByRole("button", { name: en.changeProject }));
		fireEvent.click(
			within(screen.getByRole("main")).getByRole("button", {
				name: en.changeProjectYes,
			}),
		);
		const call = await waitFor(() => {
			const f = s.calls("project.leave").at(-1);
			if (!f) throw new Error("no project.leave was sent");
			return f;
		});
		expect(s.calls("project.leave")).toHaveLength(1);
		expect(refusedBy("projectLeaveRequest", call.params)).toEqual([]);
		expect(call.params).toEqual({});
		await s.reply(call, {});
		expect(await screen.findByTestId("status")).toBeTruthy();
		expect(screen.getByTestId("status").textContent).toBe("reopening");
	});

	it("settings_shows_a_refused_change_and_clears_it_on_the_next_try", async () => {
		const { socket } = await renderApp(
			"/settings/computer",
			{ "GET /session": 204 },
			<Status />,
		);
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerStatus(s, false, 2);
		fireEvent.click(
			await screen.findByRole("button", { name: en.changeProject }),
		);
		fireEvent.click(
			within(screen.getByRole("main")).getByRole("button", {
				name: en.changeProjectYes,
			}),
		);
		const first = await waitFor(() => {
			const f = s.calls("project.leave").at(-1);
			if (!f) throw new Error("no project.leave was sent");
			return f;
		});
		await s.fail(first, -32005, "a task is still running");
		expect(await screen.findByRole("alert")).toBeTruthy();
		expect(screen.getByTestId("status").textContent).not.toBe("reopening");

		// A new attempt starts without the old error.
		fireEvent.click(
			within(screen.getByRole("main")).getByRole("button", {
				name: en.changeProjectYes,
			}),
		);
		await waitFor(() => expect(s.calls("project.leave")).toHaveLength(2));
		expect(screen.queryByRole("alert")).toBeNull();
	});

	it("shows_the_connect_page_states", async () => {
		const first = await renderApp("/events", { "GET /session": 401 });
		expect(
			await screen.findByRole("heading", { name: en.noSessionTitle }),
		).toBeTruthy();
		const code = first.container.querySelector("code");
		expect(code?.textContent).toBe("catervas serve");
		expect(screen.getByRole("button", { name: en.copy })).toBeTruthy();
		// The mark is decoration beside the wordmark, which names Catervas.
		expect(screen.getByRole("img", { name: en.brand })).toBeTruthy();
		expect(first.container.querySelectorAll('img[alt=""]').length).toBe(1);
		await expectNoAxeViolations(first.container);
		cleanup();

		const { container, hub } = await renderApp("/events");
		act(() => hub?.close());
		expect(
			await screen.findByRole("heading", { name: en.lostTitle }),
		).toBeTruthy();
		expect(screen.getByText(new RegExp(en.lostRetry))).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("lists_the_newest_events_from_the_log", async () => {
		const { container, socket } = await renderApp("/events");
		await answerStatus(socket as never, false);
		const wire = (seq: number) => ({
			seq,
			recorded_at: "2026-09-29T12:34:56.789012Z",
			kind: "team.paused",
			body: {},
		});
		// A hundred and one rows come back newest first; the page shows a hundred.
		await answerQuery(socket as never, "events.since", {
			events: Array.from({ length: 101 }, (_, i) => wire(3000 - i)),
		});
		// The page asks the log for its newest events, not for the whole of it.
		const asked = socket
			?.calls("query")
			.find((f) => f.params.name === "events.since");
		expect(asked?.params.params).toEqual({ newest_first: true, limit: 100 });
		expect(
			refusedBy("eventsSinceQuery", asked?.params as Record<string, unknown>),
		).toEqual([]);
		let rows = within(await screen.findByRole("table")).getAllByRole("row");
		expect(rows).toHaveLength(101);
		expect(
			within(rows[1] as HTMLElement).getAllByRole("cell")[0]?.textContent,
		).toBe("3000");
		// The time fits a phone: hours, minutes and seconds, with the full value on hover.
		const time = within(rows[1] as HTMLElement).getAllByRole("cell")[1];
		expect(time?.textContent).toMatch(/^\d\d:\d\d:56$/);
		expect(time?.querySelector("time")?.getAttribute("title")).toBe(
			"2026-09-29T12:34:56.789012Z",
		);
		await expectNoAxeViolations(container);

		// An event streamed afterwards is added at the top.
		act(() => socket?.event(3001));
		await waitFor(() => {
			rows = within(screen.getByRole("table")).getAllByRole("row");
			expect(
				within(rows[1] as HTMLElement).getAllByRole("cell")[0]?.textContent,
			).toBe("3001");
		});
		expect(rows).toHaveLength(101);
	});

	it("says_there_is_no_page_here", async () => {
		const { container, socket } = await renderApp("/nowhere");
		await answerStatus(socket as never, false);
		expect(
			await screen.findByRole("heading", { name: en.noPage }),
		).toBeTruthy();
		expect(
			screen.getByRole("link", { name: en.noPageHome }).getAttribute("href"),
		).toBe("/t/shop/");
		await expectNoAxeViolations(container);
	});

	it.each([
		"/requests/CTV-99",
		"/tasks/CTV-99/questions",
		"/tasks/CTV-99/plan",
		"/tasks/CTV-99/plan/edit",
		"/tasks/CTV-99/accept",
		"/tasks/CTV-99/help",
	])("says_a_task_it_cannot_read_at_%s", async (path) => {
		const { container, socket } = await renderApp(path);
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false);
		const asked = () =>
			socket.calls("query").filter((q) => q.params.name !== "serve.status");
		await waitFor(() => expect(asked().length).toBeGreaterThan(0));
		for (const q of asked())
			await socket.fail(q, -32002, "there is no task CTV-99");
		// The daemon's words never show: a task that is not there is said plainly.
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.pageNotFound,
		);
		expect(
			screen.getByRole("link", { name: en.backToToday }).getAttribute("href"),
		).toBe("/t/shop/");
		await expectNoAxeViolations(container);
	});

	it("says_a_failed_read_plainly", async () => {
		const { socket } = await renderApp("/tasks/CTV-1/help");
		if (!socket) throw new Error("no socket");
		await answerStatus(socket, false);
		const asked = () =>
			socket.calls("query").filter((q) => q.params.name !== "serve.status");
		await waitFor(() => expect(asked().length).toBeGreaterThan(0));
		for (const q of asked())
			await socket.fail(q, -32603, "the log cannot be read: disk I/O error");
		expect((await screen.findByRole("alert")).textContent).toBe(en.pageFailed);
	});
});
