import { act, render, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { expect, vi } from "vitest";
import { App } from "../app/App.tsx";
import { ConnectionProvider } from "../app/connection.tsx";
import { type FakeSocket, fakeFetch, socketsMade } from "./fake-socket.ts";

/** The whole app at `path`, with `fetch` answering `statuses`; the socket, if one opens, is open. */
export async function renderApp(
	path: string,
	statuses: Record<string, number> = { "GET /session": 204 },
	beside?: ReactNode,
) {
	const fetch = vi.fn(fakeFetch(statuses));
	vi.stubGlobal("fetch", fetch);
	const { factory, sockets } = socketsMade();
	const { container } = render(
		<ConnectionProvider socketFactory={factory}>
			{beside}
			<MemoryRouter initialEntries={[path]}>
				<App />
			</MemoryRouter>
		</ConnectionProvider>,
	);
	let socket: FakeSocket | undefined;
	if (statuses["GET /session"] === 204) {
		socket = await waitFor(() => {
			const s = sockets[0];
			if (!s) throw new Error("no socket was opened");
			return s;
		});
		const open = socket;
		act(() => open.emit("open", {}));
	}
	return { container, fetch, socket, sockets };
}

/**
 * Records event `seq`, then moves a faked clock past the 250 ms after which the page's queries
 * ask again (`REFETCH_MS` in `app/store.ts`), so the query is asked again without the test
 * waiting on a real clock that a loaded machine can stretch.
 */
export async function eventArrives(socket: FakeSocket, seq: number) {
	vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
	try {
		act(() => socket.event(seq));
		await act(() => vi.advanceTimersByTimeAsync(250));
	} finally {
		vi.useRealTimers();
	}
}

/** Answers every `name` query asked so far with `result`, once one has been asked. */
export async function answerQuery(
	socket: FakeSocket,
	name: string,
	result: unknown,
) {
	const asked = () =>
		socket.calls("query").filter((f) => f.params.name === name);
	await waitFor(() => expect(asked().length).toBeGreaterThan(0));
	for (const frame of asked()) await socket.reply(frame, result);
}

/** Answers the latest `serve.status` query, once it has been asked `count` times in all. */
export async function answerStatus(
	socket: FakeSocket,
	paused: boolean,
	count = 1,
	fields: Record<string, unknown> = {},
) {
	const asked = () =>
		socket.calls("query").filter((f) => f.params.name === "serve.status");
	await waitFor(() => expect(asked().length).toBeGreaterThanOrEqual(count));
	for (const frame of asked())
		await socket.reply(frame, {
			project_root: "/home/me/corner-bakery",
			paused,
			credential: "api_key",
			port: 7420,
			take_on_error: null,
			...fields,
		});
}
