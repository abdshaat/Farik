import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { answerCalls, NOTHING, row } from "../../test/finding.ts";
import {
	answerQuery,
	answerStatus,
	renderApp,
} from "../../test/render-app.tsx";

const REPOS = {
	repos: [
		{
			name: "tidepool-web",
			path: "/home/me/code/tidepool-web",
			integration_branch: null,
			missing: false,
		},
		{
			name: "tidepool-api",
			path: "/home/me/code/tidepool-api",
			integration_branch: null,
			missing: false,
		},
		{
			name: "tidepool-infra",
			path: "/home/me/code/tidepool-infra",
			integration_branch: null,
			missing: false,
		},
	],
};

describe("repositories", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
		localStorage.clear();
	});

	it("takes_a_repository_out_of_a_team", async () => {
		const { socket, hub } = await renderApp("/settings/repositories");
		const s = socket as FakeSocket;
		const h = hub as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "team.get", {
			team: { name: "Tidepool", agents: [] },
			agents: [],
			max_agents: 7,
		});
		await answerQuery(s, "repos.list", REPOS);
		answerCalls(s, { "repo.remove": {} });
		answerCalls(h, {
			"finding.list": {
				...NOTHING,
				main: ["tidepool-web", "tidepool-api", "tidepool-infra"].map((name) =>
					row({
						key: `/home/me/code/${name}`,
						name,
						folders: [`/home/me/code/${name}`],
						already_team: null,
					}),
				),
			},
			"linking.reject": {},
			"finding.choose": {},
		});

		const table = await screen.findByRole("table");
		const infra = within(table).getByText("tidepool-infra").closest("tr");
		expect(infra).toBeTruthy();
		expect(within(table).getByText("~/code/tidepool-infra")).toBeTruthy();
		fireEvent.click(
			within(infra as HTMLElement).getByRole("button", {
				name: "This doesn’t belong here",
			}),
		);

		// It asks what becomes of the repository, Watch it chosen.
		const dialog = await screen.findByRole("dialog", {
			name: "Take tidepool-infra out of Tidepool?",
		});
		expect(
			within(dialog).getByText("The team stops working in it."),
		).toBeTruthy();
		expect(
			(
				within(dialog).getByRole("radio", {
					name: /Watch it/,
				}) as HTMLInputElement
			).checked,
		).toBe(true);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Take it out" }),
		);

		await waitFor(() =>
			expect(s.calls("repo.remove").map((f) => f.params)).toEqual([
				{ name: "tidepool-infra" },
			]),
		);
		// The link is kept as rejected against each repository that stays, and the choice is Watch.
		await waitFor(() =>
			expect(h.calls("linking.reject").map((f) => f.params)).toEqual([
				{
					key: "/home/me/code/tidepool-infra",
					others: ["/home/me/code/tidepool-web", "/home/me/code/tidepool-api"],
				},
			]),
		);
		await waitFor(() =>
			expect(h.calls("finding.choose").map((f) => f.params)).toEqual([
				{ key: "/home/me/code/tidepool-infra", choice: "watch" },
			]),
		);
	});
});
