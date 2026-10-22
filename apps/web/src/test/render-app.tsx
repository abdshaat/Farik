import { act, render, waitFor } from "@testing-library/react";
import { type ReactNode, useEffect } from "react";
import { MemoryRouter, useLocation, useNavigate } from "react-router";
import { expect, vi } from "vitest";
import { App } from "../app/App.tsx";
import { ConnectionProvider } from "../app/connection.tsx";
import { type FakeSocket, fakeFetch, socketsMade } from "./fake-socket.ts";

/** The address the app is at, as its router has it. */
export function where(): string {
	return document.body.dataset.where ?? "";
}

/** The team a path of the old, one-team addresses is rendered under. */
export const TEAM = "shop";

/** Whether `path` is one the computer's pages or the setup's first steps own, not a team's. */
const HUB_ONLY =
	/^\/(connect|teams|computer|find|setup\/(computer|account|project|where))/;
/** The setup's later steps talk to the team just taken on, through the current team's socket. */
const CURRENT =
	/^\/setup\/(scan|team|permissions|spending|finish|advanced|ready)/;

/** Where `path` is rendered: a team's page of the one-team days goes under `/t/shop`. */
export function placed(path: string): string {
	if (HUB_ONLY.test(path) || CURRENT.test(path) || path.startsWith("/t/"))
		return path;
	return `/t/${TEAM}${path === "/" ? "/" : path}`;
}

/**
 * The whole app at `path`, with `fetch` answering `statuses`; the sockets, if they open, are open.
 * A page of a team (`/board`) is rendered in the team `shop`, and `socket` is that team's; a
 * wizard step has the socket it talks to. `hub` is the hub's, and `raw` renders `path` as given.
 */
export async function renderApp(
	path: string,
	statuses: Record<string, number> = { "GET /session": 204 },
	beside?: ReactNode,
	options: { raw?: boolean } = {},
) {
	const fetch = vi.fn(fakeFetch(statuses));
	vi.stubGlobal("fetch", fetch);
	const { factory, sockets } = socketsMade();
	let go: (to: string, state?: unknown) => void = () => {};
	function Capture() {
		const navigate = useNavigate();
		go = (to, state) => void navigate(to, { state });
		const { pathname } = useLocation();
		useEffect(() => {
			document.body.dataset.where = pathname;
		}, [pathname]);
		return null;
	}
	const at = options.raw ? path : placed(path);
	const { container } = render(
		<ConnectionProvider socketFactory={factory}>
			{beside}
			<MemoryRouter initialEntries={[at]}>
				<Capture />
				<App />
			</MemoryRouter>
		</ConnectionProvider>,
	);
	const find = (route: string) =>
		sockets.find((s) => new URL(s.url).pathname === route);
	const opened = new Set<FakeSocket>();
	/** Opens every socket made so far that is not open yet. */
	const open = () => {
		for (const s of sockets)
			if (!opened.has(s)) {
				opened.add(s);
				act(() => s.emit("open", {}));
			}
	};
	let socket: FakeSocket | undefined;
	let hub: FakeSocket | undefined;
	if (statuses["GET /session"] === 204) {
		hub = await waitFor(() => {
			const s = find("/hub/rpc");
			if (!s) throw new Error("no socket was opened");
			return s;
		});
		open();
		const wanted = /^\/t\//.test(at)
			? `/t/${at.split("/")[2]}/rpc`
			: CURRENT.test(at)
				? "/rpc"
				: "/hub/rpc";
		socket = await waitFor(() => {
			const s = find(wanted);
			if (!s) throw new Error(`no socket to ${wanted}`);
			return s;
		});
		open();
	}
	return {
		container,
		fetch,
		socket,
		hub,
		sockets,
		/** Goes to `to` inside the app. */
		go: (to: string, state?: unknown) => act(() => go(to, state)),
		/** Opens the sockets made since. */
		open,
	};
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
			signed_in: true,
			port: 7420,
			take_on_error: null,
			...fields,
		});
}

/** A `teams.list` answer holding `slugs`, each running, the first the current one. */
export function teamList(...slugs: string[]) {
	return {
		teams: slugs.map((slug) => ({
			slug,
			name: slug,
			root: `/home/me/${slug}`,
			status: "running",
			working_agents: 0,
			waiting: 0,
			today_usd: 0,
			month_usd: 0,
			undoable: false,
		})),
		current: slugs[0] ?? null,
		account_asleep_until: null,
	};
}

/**
 * Where the hub sends "/" when `slug` is the only team: answers the hub's two queries, opens the
 * team's socket, and answers the team's status. Gives the team's socket.
 */
export async function intoTeam(
	hub: FakeSocket,
	sockets: FakeSocket[],
	paused = false,
	slug = TEAM,
) {
	await answerQuery(hub, "teams.list", teamList(slug));
	await answerStatus(hub, false);
	const team = await waitFor(() => {
		const s = sockets.find(
			(one) => new URL(one.url).pathname === `/t/${slug}/rpc`,
		);
		if (!s) throw new Error(`no socket to ${slug}`);
		return s;
	});
	act(() => team.emit("open", {}));
	await answerStatus(team, paused);
	return team;
}
