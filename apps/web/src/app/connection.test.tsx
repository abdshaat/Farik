import { act, render, screen, waitFor } from "@testing-library/react";
import { BrowserRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { fakeFetch, socketsMade } from "../test/fake-socket.ts";
import { answerQuery, answerStatus } from "../test/render-app.tsx";
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
		const { factory, sockets } = socketsMade();
		render(
			<ConnectionProvider socketFactory={factory}>
				<BrowserRouter>
					<App />
				</BrowserRouter>
			</ConnectionProvider>,
		);
		expect(location.hash).toBe("");
		const socket = await waitFor(() => {
			const s = sockets[0];
			if (!s) throw new Error("no socket was opened");
			return s;
		});
		act(() => socket.emit("open", {}));
		// "/" is the only team's Today, once Catervas says it has a project and its team.
		await answerStatus(socket, false);
		await answerQuery(socket, "teams.list", {
			teams: [
				{
					slug: "shop",
					name: "shop",
					root: "/home/me/shop",
					status: "running",
					working_agents: 0,
					waiting: 0,
					today_usd: 0,
					month_usd: 0,
				},
			],
			current: "shop",
			account_asleep_until: null,
		});
		const team = await waitFor(() => {
			const s = sockets.find((one) => one.url.endsWith("/t/shop/rpc"));
			if (!s) throw new Error("no socket to the team");
			return s;
		});
		act(() => team.emit("open", {}));
		await answerStatus(team, false);
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
		expect(location.pathname).toBe("/t/shop/");
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

	it("redeems_a_link_opened_in_the_same_tab", async () => {
		history.replaceState(null, "", "/connect");
		const fetch = vi.fn(
			fakeFetch({ "POST /connect": 204, "GET /session": 401 }),
		);
		vi.stubGlobal("fetch", fetch);
		render(
			<ConnectionProvider socketFactory={socketsMade().factory}>
				<Status />
			</ConnectionProvider>,
		);
		await waitFor(() => expect(status()).toBe("no_session"));
		expect(fetch).not.toHaveBeenCalledWith("/connect", expect.anything());
		// A second link, opened in this tab: only the hash changes.
		act(() => {
			location.hash = CODE;
			dispatchEvent(new HashChangeEvent("hashchange"));
		});
		await waitFor(() =>
			expect(fetch).toHaveBeenCalledWith(
				"/connect",
				expect.objectContaining({
					method: "POST",
					body: JSON.stringify({ code: CODE }),
				}),
			),
		);
	});

	it("shows_opening_while_it_connects", async () => {
		vi.stubGlobal("fetch", () => new Promise(() => {}));
		render(
			<ConnectionProvider socketFactory={socketsMade().factory}>
				<BrowserRouter>
					<App />
				</BrowserRouter>
			</ConnectionProvider>,
		);
		expect(screen.getByText(en.openingCatervas)).toBeTruthy();
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
		// The used link goes to "/", which then asks Catervas where to go.
		expect(location.pathname).toBe("/");
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

	it("subscribes_from_now_and_resumes_where_it_was", async () => {
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
		expect(first.url).toBe(`ws://${location.host}/hub/rpc`);
		act(() => first.emit("open", {}));
		expect(status()).toBe("open");
		expect(first.calls("subscribe")[0]?.params).toEqual({});
		const subscribed = first.calls("subscribe")[0];
		if (!subscribed) throw new Error("no subscribe was sent");
		await first.reply(subscribed, { head: 2 });
		act(() => {
			for (const seq of [3, 4]) first.event(seq);
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
		expect(second.calls("subscribe")[0]?.params).toEqual({ from_seq: 4 });
	});

	it("notifies_in_the_browser_when_the_digest_is_due", async () => {
		const shown: { title: string; options: NotificationOptions }[] = [];
		vi.stubGlobal(
			"Notification",
			Object.assign(
				function Notification(title: string, options: NotificationOptions) {
					shown.push({ title, options });
				},
				{ permission: "granted", requestPermission: async () => "granted" },
			),
		);
		vi.stubGlobal("fetch", vi.fn(fakeFetch({ "GET /session": 204 })));
		const { factory, sockets } = socketsMade();
		render(
			<ConnectionProvider socketFactory={factory}>
				<Status />
			</ConnectionProvider>,
		);
		const socket = await waitFor(() => {
			const s = sockets[0];
			if (!s) throw new Error("no socket was opened");
			return s;
		});
		act(() => socket.emit("open", {}));
		await waitFor(() => expect(status()).toBe("open"));
		const due = {
			at: "2026-10-22T09:00:00Z",
			count: 9,
			headline: "A price check failed, and Tidepool's last deploy failed.",
			time: "09:00",
		};
		act(() => socket.notify("reminders.due", due));
		await waitFor(() => expect(shown).toHaveLength(1));
		expect(shown[0]?.title).toBe("Catervas · 09:00");
		expect(shown[0]?.options.body).toBe(
			"9 things need you. A price check failed, and Tidepool's last deploy failed.",
		);
		// The same digest, told again, shows nothing more; the next day's does.
		act(() => socket.notify("reminders.due", due));
		await act(async () => {});
		expect(shown).toHaveLength(1);
		act(() =>
			socket.notify("reminders.due", { ...due, at: "2026-10-23T09:00:00Z" }),
		);
		await waitFor(() => expect(shown).toHaveLength(2));
	});
});
