import { act, render, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { expect, vi } from "vitest";
import { App } from "../app/App.tsx";
import { ConnectionProvider } from "../app/connection.tsx";
import { type FakeSocket, fakeFetch, socketsMade } from "./fake-socket.ts";

/** The whole app at `path`, with `fetch` answering `statuses`; the socket, if one opens, is open. */
export async function renderApp(
	path: string,
	statuses: Record<string, number> = { "GET /session": 204 },
) {
	const fetch = vi.fn(fakeFetch(statuses));
	vi.stubGlobal("fetch", fetch);
	const { factory, sockets } = socketsMade();
	const { container } = render(
		<ConnectionProvider socketFactory={factory}>
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
	return { container, fetch, socket };
}

/** Answers the latest `serve.status` query, once it has been asked `count` times in all. */
export async function answerStatus(
	socket: FakeSocket,
	paused: boolean,
	count = 1,
) {
	const asked = () =>
		socket.calls("query").filter((f) => f.params.name === "serve.status");
	await waitFor(() => expect(asked().length).toBeGreaterThanOrEqual(count));
	for (const frame of asked())
		act(() =>
			socket.reply(frame, {
				project_root: "/home/me/corner-bakery",
				paused,
				credential: "api_key",
				port: 7420,
			}),
		);
}
