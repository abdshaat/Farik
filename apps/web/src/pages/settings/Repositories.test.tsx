import {
	act,
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { answerCalls, NOTHING, row } from "../../test/finding.ts";
import {
	answerQuery,
	answerStatus,
	eventArrives,
	renderApp,
} from "../../test/render-app.tsx";
import { refusedBy } from "../../test/schema.ts";

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

	/** Answers the `folders.list` query for `path` with `entries`, once it is asked. */
	async function listing(
		s: FakeSocket,
		path: string,
		entries: { name: string; git: boolean }[],
	) {
		const asked = () =>
			s
				.calls("query")
				.filter(
					(f) =>
						f.params.name === "folders.list" &&
						(f.params.params as { path: string }).path === path,
				);
		await waitFor(() => expect(asked().length).toBeGreaterThan(0));
		for (const frame of asked())
			await s.reply(frame, {
				path,
				parent: path === "" ? null : "",
				entries,
			});
	}

	/** The page of a one-repository project, with its dialog to add a repository open. */
	async function dialog(blockers: unknown) {
		const rendered = await renderApp("/settings/repositories");
		const s = rendered.socket as FakeSocket;
		await answerStatus(s, false, 1, {
			project_root: "/home/me/Projects/corner-bakery",
		});
		await answerQuery(s, "team.get", {
			team: { name: "Corner Bakery", agents: [] },
			agents: [],
			max_agents: 7,
		});
		await answerQuery(s, "repos.list", { repos: null });
		// The page asks again, for the project's folder.
		await answerStatus(s, false, 2, {
			project_root: "/home/me/Projects/corner-bakery",
		});
		expect(
			await screen.findByText(
				/Your team works in one repository, ~\/Projects\/corner-bakery\./,
			),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: en.reposAdd }));
		const open = await screen.findByRole("dialog", {
			name: "Add a second repository to corner-bakery",
		});
		await answerQuery(s, "project.convert_blockers", blockers);
		return { ...rendered, s, open };
	}

	const FREE = { tasks: [], catervafication: false, folder_changes: [] };

	it("adds_a_second_repository", async () => {
		const { s, open } = await dialog({
			tasks: ["CTV-1"],
			catervafication: false,
			folder_changes: [],
		});

		// It says in words what changes, before anything is asked of the owner.
		expect(
			within(open).getByText(
				/Your team’s notes and plans move out of corner-bakery into a folder on this computer only/,
			),
		).toBeTruthy();
		expect(within(open).getByText("~/Catervas/corner-bakery")).toBeTruthy();
		expect(
			within(open).getByText(/on the branch catervas\/moved-out/),
		).toBeTruthy();
		expect(
			within(open).getByText(/Nothing else in corner-bakery changes/),
		).toBeTruthy();
		expect(
			(
				within(open).getByLabelText(
					"Name corner-bakery in this project",
				) as HTMLInputElement
			).value,
		).toBe("corner-bakery");

		// While the team has work there, the dialog says so and cannot go on.
		expect(
			within(open).getByText(
				"Your team is still working in corner-bakery: CTV-1. Let them finish, or cancel them, before you add a repository.",
			),
		).toBeTruthy();
		const next = within(open).getByRole("button", { name: en.reposContinue });
		expect((next as HTMLButtonElement).disabled).toBe(true);

		// With nothing in the way, it goes on to the second repository.
		await eventArrives(s, 5);
		await answerQuery(s, "project.convert_blockers", FREE);
		await waitFor(() =>
			expect(
				(
					within(open).getByRole("button", {
						name: en.reposContinue,
					}) as HTMLButtonElement
				).disabled,
			).toBe(false),
		);
		fireEvent.click(
			within(open).getByRole("button", { name: en.reposContinue }),
		);
		await listing(s, "", [{ name: "Projects", git: false }]);
		fireEvent.click(await within(open).findByText("Projects"));
		fireEvent.click(within(open).getByRole("button", { name: "Open folder" }));
		await listing(s, "Projects", [{ name: "web", git: true }]);
		fireEvent.click(await within(open).findByText("web"));
		fireEvent.click(
			within(open).getByRole("button", { name: "Use this folder" }),
		);
		fireEvent.click(
			within(open).getByRole("button", { name: en.reposContinue }),
		);

		const call = await waitFor(() => {
			const f = s.calls("project.convert").at(-1);
			if (!f) throw new Error("no project.convert was sent");
			return f;
		});
		expect(call.params).toEqual({
			first_name: "corner-bakery",
			parent: "Catervas",
			name: "corner-bakery",
			second: { name: "web", path: "Projects/web" },
		});
		expect(refusedBy("projectConvertRequest", call.params)).toEqual([]);
		await s.reply(call, {});

		// The page says the notes are moving, and goes on trying to reach the team for longer.
		expect(await screen.findByText(en.movingTitle)).toBeTruthy();
		const fetches = vi.mocked(fetch).mock.calls.length;
		act(() => s.close());
		await waitFor(() =>
			expect(vi.mocked(fetch).mock.calls.length).toBeGreaterThan(fetches),
		);
	});

	it("shows_a_refused_move", async () => {
		const { s, open } = await dialog(FREE);
		await waitFor(() =>
			expect(
				(
					within(open).getByRole("button", {
						name: en.reposContinue,
					}) as HTMLButtonElement
				).disabled,
			).toBe(false),
		);
		fireEvent.click(
			within(open).getByRole("button", { name: en.reposContinue }),
		);
		await listing(s, "", [{ name: "web", git: true }]);
		fireEvent.click(await within(open).findByText("web"));
		fireEvent.click(
			within(open).getByRole("button", { name: "Use this folder" }),
		);
		const refuse = async (code: string, path: string, message: string) => {
			fireEvent.click(
				within(open).getByRole("button", { name: en.reposContinue }),
			);
			const sent = await waitFor(() => {
				const f = s.calls("project.convert").at(-1);
				if (!f) throw new Error("no project.convert was sent");
				return f;
			});
			await s.fail(sent, -32005, message, {
				errors: [{ path, message, code }],
			});
		};

		await refuse(
			"work_in_progress",
			"/",
			"Your team is still working in corner-bakery: CTV-3. Let them finish, or cancel them, before you add a repository.",
		);
		expect(
			await within(open).findByText(
				"Your team is still working in corner-bakery: CTV-3. Let them finish, or cancel them, before you add a repository.",
			),
		).toBeTruthy();
		// The dialog stays, and the next refusals read in their own words.
		await refuse(
			"folder_there",
			"/name",
			"a folder with that name is already there",
		);
		expect(
			await within(open).findByText(
				"A folder with that name is already there. Choose another name or place.",
			),
		).toBeTruthy();
		await refuse(
			"not_one_repo",
			"/",
			"This project has several repositories already.",
		);
		expect(
			await within(open).findByText(
				"This project has several repositories already.",
			),
		).toBeTruthy();
		expect(screen.queryByText(en.movingTitle)).toBeNull();
	});

	it("shows_a_failed_move", async () => {
		const { socket } = await renderApp("/settings/repositories");
		const s = socket as FakeSocket;
		await answerStatus(s, false, 1, {
			project_root: "/home/me/Projects/corner-bakery",
			take_on_error:
				"the branch catervas/moved-out could not be made: git refused",
		});
		await answerQuery(s, "team.get", {
			team: { name: "Corner Bakery", agents: [] },
			agents: [],
			max_agents: 7,
		});
		await answerQuery(s, "repos.list", { repos: null });
		await answerStatus(s, false, 2, {
			project_root: "/home/me/Projects/corner-bakery",
			take_on_error:
				"the branch catervas/moved-out could not be made: git refused",
		});

		expect(
			await screen.findByText(
				"Catervas could not add the repository: the branch catervas/moved-out could not be made: git refused",
			),
		).toBeTruthy();
	});
});
