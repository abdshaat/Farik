import { act, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fakeFetch, socketsMade } from "../test/fake-socket.ts";
import { ConnectionProvider } from "./connection.tsx";
import { useEvents, useQuery } from "./store.ts";

const flush = () => act(() => vi.advanceTimersByTimeAsync(0));

async function openWith(children: React.ReactNode) {
	vi.stubGlobal("fetch", fakeFetch({ "GET /session": 204 }));
	const { factory, sockets } = socketsMade();
	render(
		<ConnectionProvider socketFactory={factory}>{children}</ConnectionProvider>,
	);
	await flush();
	const socket = sockets[0];
	if (!socket) throw new Error("no socket was opened");
	act(() => socket.emit("open", {}));
	return socket;
}

describe("store", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("keeps_the_last_five_hundred_events", async () => {
		vi.useFakeTimers();
		function Seqs() {
			return (
				<p data-testid="seqs">
					{useEvents()
						.map((e) => e.seq)
						.join(",")}
				</p>
			);
		}
		const socket = await openWith(<Seqs />);
		act(() => {
			for (let seq = 1; seq <= 600; seq++) socket.event(seq);
		});
		const seqs = screen.getByTestId("seqs").textContent?.split(",").map(Number);
		expect(seqs).toHaveLength(500);
		expect(seqs?.[0]).toBe(101);
		expect(seqs?.at(-1)).toBe(600);
	});

	it("queries_again_after_an_event", async () => {
		vi.useFakeTimers();
		function Status() {
			useQuery("serve.status", {});
			return null;
		}
		const socket = await openWith(<Status />);
		const queries = () =>
			socket.calls("query").filter((f) => f.params.name === "serve.status");
		expect(queries()).toHaveLength(1);
		act(() => {
			for (let seq = 1; seq <= 10; seq++) socket.event(seq);
		});
		await act(() => vi.advanceTimersByTimeAsync(250));
		expect(queries()).toHaveLength(2);
	});
});
