import { act, render, screen, waitFor } from "@testing-library/react";
import { BrowserRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { fakeFetch, socketsMade } from "../test/fake-socket.ts";
import { App } from "./App.tsx";
import { ConnectionProvider, useConnection } from "./connection.tsx";

const CODE = "0123456789abcdef".repeat(4);

function Status() {
	return <p data-testid="status">{useConnection().status}</p>;
}
const status = () => screen.getByTestId("status").textContent;
const flush = () => act(() => vi.advanceTimersByTimeAsync(0));

describe("connection", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
		history.replaceState(null, "", "/");
	});

	it("trades_the_code_in_the_link_for_a_session", async () => {
		history.replaceState(null, "", `/connect#${CODE}`);
		const fetch = vi.fn(
			fakeFetch({ "POST /connect": 204, "GET /session": 204 }),
		);
		vi.stubGlobal("fetch", fetch);
		const { factory } = socketsMade();
		render(
			<ConnectionProvider socketFactory={factory}>
				<BrowserRouter>
					<App />
				</BrowserRouter>
			</ConnectionProvider>,
		);
		expect(location.hash).toBe("");
		// "/" redirects to the event list until Today exists.
		await waitFor(() => expect(location.pathname).toBe("/events"));
		expect(fetch).toHaveBeenCalledWith(
			"/connect",
			expect.objectContaining({
				method: "POST",
				body: JSON.stringify({ code: CODE }),
			}),
		);
	});

	it("says_the_link_was_used", async () => {
		history.replaceState(null, "", `/connect#${CODE}`);
		vi.stubGlobal(
			"fetch",
			fakeFetch({ "POST /connect": 401, "GET /session": 401 }),
		);
		render(
			<ConnectionProvider socketFactory={socketsMade().factory}>
				<BrowserRouter>
					<App />
				</BrowserRouter>
			</ConnectionProvider>,
		);
		expect(await screen.findByText(en.linkUsed)).toBeTruthy();
		expect(location.pathname).toBe("/connect");
	});

	it("sends_a_used_link_home_when_the_browser_has_a_session", async () => {
		vi.useFakeTimers();
		history.replaceState(null, "", `/connect#${CODE}`);
		vi.stubGlobal(
			"fetch",
			fakeFetch({ "POST /connect": 401, "GET /session": 204 }),
		);
		const { factory, sockets } = socketsMade();
		function LinkUsed() {
			return <p data-testid="used">{String(useConnection().linkUsed)}</p>;
		}
		render(
			<ConnectionProvider socketFactory={factory}>
				<LinkUsed />
				<BrowserRouter>
					<App />
				</BrowserRouter>
			</ConnectionProvider>,
		);
		await flush();
		const socket = sockets[0];
		if (!socket) throw new Error("no socket was opened");
		act(() => socket.emit("open", {}));
		expect(screen.getByTestId("used").textContent).toBe("false");
		// "/" redirects to the event list until Today exists.
		expect(location.pathname).toBe("/events");
	});

	it("asks_for_a_start_link_without_a_session", async () => {
		const fetch = vi.fn(fakeFetch({ "GET /session": 401 }));
		vi.stubGlobal("fetch", fetch);
		const { factory, sockets } = socketsMade();
		render(
			<ConnectionProvider socketFactory={factory}>
				<Status />
			</ConnectionProvider>,
		);
		await waitFor(() => expect(status()).toBe("no_session"));
		expect(fetch).toHaveBeenCalledWith(
			"/session",
			expect.objectContaining({ credentials: "same-origin" }),
		);
		expect(sockets).toEqual([]);
	});

	it("reconnects_after_the_connection_is_lost", async () => {
		vi.useFakeTimers();
		const fetch = vi.fn(fakeFetch({ "GET /session": 204 }));
		vi.stubGlobal("fetch", fetch);
		const { factory, sockets } = socketsMade();
		render(
			<ConnectionProvider socketFactory={factory}>
				<Status />
			</ConnectionProvider>,
		);
		await flush();
		const first = sockets[0];
		if (!first) throw new Error("no socket was opened");
		expect(first.url).toBe(`ws://${location.host}/rpc`);
		act(() => first.emit("open", {}));
		expect(status()).toBe("open");
		expect(first.calls("subscribe")[0]?.params).toEqual({ from_seq: 0 });
		act(() => {
			for (const seq of [1, 2, 3]) first.event(seq);
		});

		act(() => first.close());
		expect(status()).toBe("lost");
		await act(() => vi.advanceTimersByTimeAsync(4999));
		expect(fetch).toHaveBeenCalledTimes(1);
		expect(sockets).toHaveLength(1);
		await act(() => vi.advanceTimersByTimeAsync(1));
		expect(fetch).toHaveBeenCalledTimes(2);
		const second = sockets[1];
		if (!second) throw new Error("no socket was reopened");
		act(() => second.emit("open", {}));
		expect(status()).toBe("open");
		expect(second.calls("subscribe")[0]?.params).toEqual({ from_seq: 3 });
	});
});
