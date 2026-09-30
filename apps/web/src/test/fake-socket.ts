import type { SocketLike } from "@farik/protocol-client";
import { act } from "@testing-library/react";

type Frame = { id: number; method: string; params: Record<string, unknown> };

/** A socket the test drives: it records what the client sends and plays the daemon's side. */
export class FakeSocket implements SocketLike {
	sent: Frame[] = [];
	private handlers: Record<string, ((e: { data: unknown }) => void)[]> = {};
	url: string;
	constructor(url: string) {
		this.url = url;
	}
	send(data: string) {
		this.sent.push(JSON.parse(data));
	}
	close() {
		this.emit("close", {});
	}
	addEventListener(type: string, fn: (e: { data: unknown }) => void) {
		const list = this.handlers[type] ?? [];
		this.handlers[type] = list;
		list.push(fn);
	}
	emit(type: string, e: object) {
		for (const fn of this.handlers[type] ?? []) fn(e as { data: unknown });
	}
	event(seq: number) {
		this.emit("message", {
			data: JSON.stringify({
				jsonrpc: "2.0",
				method: "event",
				params: {
					event: {
						seq,
						recorded_at: "2026-09-29T12:34:56.789012Z",
						kind: "team.paused",
						body: {},
					},
				},
			}),
		});
	}
	/** Answers a request the client sent, as the daemon would, once the page has taken the answer in. */
	reply(frame: Frame, result: unknown) {
		return this.settle({ jsonrpc: "2.0", id: frame.id, result });
	}
	/** Refuses a request the client sent, as the daemon would, once the page has taken the refusal in. */
	fail(frame: Frame, code: number, message: string, data?: unknown) {
		return this.settle({
			jsonrpc: "2.0",
			id: frame.id,
			error: { code, message, ...(data === undefined ? {} : { data }) },
		});
	}
	/**
	 * Sends a response and waits until every render and effect it causes has run.
	 * A response resolves the client's promise, so the page's update comes a microtask later:
	 * a synchronous `act` has already returned by then, and React renders it on its own
	 * scheduler, where the DOM can show the answer before the effects that follow it have run.
	 * An async `act` holds every such update inside itself: it flushes React's work, then
	 * waits a macrotask and flushes again, until nothing is left.
	 */
	private async settle(response: object) {
		await act(async () => {
			this.emit("message", { data: JSON.stringify(response) });
		});
	}
	calls(method: string): Frame[] {
		return this.sent.filter((f) => f.method === method);
	}
}

/** A `socketFactory` that keeps every socket it makes, newest last. */
export function socketsMade(): {
	sockets: FakeSocket[];
	factory: (url: string) => FakeSocket;
} {
	const sockets: FakeSocket[] = [];
	return {
		sockets,
		factory: (url) => {
			const s = new FakeSocket(url);
			sockets.push(s);
			return s;
		},
	};
}

/** A `fetch` that answers each "METHOD /path" with its status and anything else with 404. */
export function fakeFetch(statuses: Record<string, number>) {
	return async (url: string, init?: RequestInit) =>
		new Response(null, {
			status: statuses[`${init?.method ?? "GET"} ${url}`] ?? 404,
		});
}
