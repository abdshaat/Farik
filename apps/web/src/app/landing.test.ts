import { cleanup, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerStatus, renderApp } from "../test/render-app.tsx";
import { landing } from "./landing.ts";

const status = (fields: object) => ({
	projectRoot: "/home/me/corner-bakery",
	paused: false,
	credential: "api_key",
	port: 7420,
	takeOnError: null,
	...fields,
});

describe("landing", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("lands_where_the_status_says", async () => {
		expect(landing(status({ projectRoot: null }))).toBe("/setup/computer");
		expect(
			landing(status({ projectRoot: null, takeOnError: "not a folder" })),
		).toBe("/setup/project");
		expect(landing(status({ setupPending: true }))).toBe("/setup/scan");
		expect(landing(status({ setupPending: false }))).toBe("/");

		// Before serve.status answers, "/" shows nothing and asks nothing of the project.
		const { container, socket } = await renderApp("/");
		const s = socket as FakeSocket;
		await waitFor(() => expect(s.calls("query").length).toBeGreaterThan(0));
		expect(container.textContent).toBe("");
		expect(s.calls("query").map((q) => q.params.name as string)).toEqual([
			"serve.status",
		]);
		await answerStatus(s, false);
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
		cleanup();

		// During setup, a deep link goes where "/" would.
		const deep = await renderApp("/settings");
		await answerStatus(deep.socket as FakeSocket, false, 1, {
			setup_pending: true,
		});
		expect(
			await screen.findByRole("heading", { name: en.scanTitle }),
		).toBeTruthy();
	});
});
