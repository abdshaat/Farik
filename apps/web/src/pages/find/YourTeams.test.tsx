import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { answerCalls, IDLE, NOTHING, row } from "../../test/finding.ts";
import {
	answerQuery,
	renderApp,
	teamList,
	where,
} from "../../test/render-app.tsx";

const WEB = {
	key: "/home/me/tidepool-web",
	name: "tidepool-web",
	folder: "~/tidepool-web",
	services: [],
};
const API = {
	key: "/home/me/tidepool-api",
	name: "tidepool-api",
	folder: "~/tidepool-api",
	services: [],
};
const WATCH = {
	key: "/home/me/price-watch",
	name: "price-watch",
	folder: "~/price-watch",
	services: [],
};
const BOOKS = {
	key: "Bookkeeping (Zoho Books)",
	name: "Bookkeeping (Zoho Books)",
	services: ["Zoho Books", "Excel"],
};

const TEMPLATES = [
	{ slug: "software-product", name: "Software product" },
	{ slug: "online-store", name: "Online store" },
	{ slug: "financial-analysis", name: "Financial analysis" },
];

const card = (
	fields: { id: string; name: string } & Record<string, unknown>,
) => ({
	kind: "new_team",
	jobs: [] as string[],
	why: [] as string[],
	template: "software-product",
	from_claude: false,
	...fields,
});

const CARDS = [
	card({
		id: "c1",
		name: "tidepool",
		projects: [WEB, API],
		jobs: ["Nightly cleanup", "Weekly database backup"],
		why: ["tidepool-web uses tidepool-api's code."],
	}),
	card({ id: "c2", name: "price-watch", projects: [WATCH] }),
	card({
		id: "c3",
		name: "Bookkeeping",
		projects: [BOOKS],
		template: "financial-analysis",
		folder: "~/Catervas/bookkeeping",
	}),
];

const articleOf = (name: string) =>
	screen
		.getByRole("heading", { name, level: 2 })
		.closest("article") as HTMLElement;

describe("your teams", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
		localStorage.clear();
	});

	it("checks_each_team_before_creating", async () => {
		const { hub } = await renderApp("/find/teams");
		const h = hub as FakeSocket;
		let cards: unknown[] = CARDS;
		let created = false;
		const table: Record<string, unknown> = {
			"proposal.get": () => ({ cards, templates: TEMPLATES }),
			"proposal.names_estimate": { usd: 0.0213, input_tokens: 3800 },
			"proposal.edit": {},
			"proposal.move_out": {},
			"proposal.revert": {},
			"proposal.names": () => {
				cards = CARDS.map((each) =>
					each.id === "c1"
						? {
								...each,
								name: "Tidepool",
								purpose: "Surf forecasts: the app, its API and its servers.",
								from_claude: true,
							}
						: each,
				);
				return { cost_usd: 0.02, cards: [] };
			},
			"proposal.create": () => {
				created = true;
				return {
					made: CARDS.map((each) => ({
						id: each.id,
						slug: each.id,
						name: each.name,
						root: `/home/me/${each.id}`,
					})),
					added: [],
					failed: [],
					split: [],
				};
			},
			"finding.list": {
				...NOTHING,
				main: [
					row({
						key: "/home/me/recipe-cards",
						name: "recipe-cards",
						suggested: "watch",
					}),
					row({
						key: "/home/me/tidepool-web",
						name: "tidepool-web",
						suggested: "team",
					}),
				],
			},
			"finding.status": IDLE,
		};
		answerCalls(h, table);

		// Each card: its name, repositories, why, jobs and team.
		expect(
			await screen.findByRole("heading", { name: "Your teams", level: 1 }),
		).toBeTruthy();
		expect(screen.getByText("Check each one. Change anything.")).toBeTruthy();
		const tidepool = within(await waitFor(() => articleOf("tidepool")));
		expect(tidepool.getByText("tidepool-web")).toBeTruthy();
		expect(tidepool.getByText("tidepool-api")).toBeTruthy();
		expect(
			tidepool.getByText("tidepool-web uses tidepool-api's code."),
		).toBeTruthy();
		expect(
			tidepool.getByText("Jobs: Nightly cleanup, Weekly database backup"),
		).toBeTruthy();
		expect(
			(tidepool.getByRole("combobox", { name: "Team" }) as HTMLSelectElement)
				.value,
		).toBe("software-product");
		const books = within(articleOf("Bookkeeping"));
		expect(books.getByText("~/Catervas/bookkeeping")).toBeTruthy();
		expect(books.getByText("Zoho Books")).toBeTruthy();
		// The watched line comes from its own query, which may answer after the cards.
		expect(
			await screen.findByText("Watched, no team: recipe-cards."),
		).toBeTruthy();
		expect(
			screen.getByText(
				"They start paused. Nothing is spent until you start one.",
			),
		).toBeTruthy();

		// Changing the team sends the edit.
		fireEvent.change(tidepool.getByRole("combobox", { name: "Team" }), {
			target: { value: "online-store" },
		});
		await waitFor(() =>
			expect(h.calls("proposal.edit").map((f) => f.params)).toEqual([
				{ id: "c1", template: "online-store" },
			]),
		);

		// A project is moved out of a card that has several.
		fireEvent.click(
			tidepool.getByRole("button", { name: "Move a project out" }),
		);
		fireEvent.click(
			tidepool.getByRole("button", { name: "Move tidepool-api out" }),
		);
		await waitFor(() =>
			expect(h.calls("proposal.move_out").map((f) => f.params)).toEqual([
				{ id: "c1", key: API.key },
			]),
		);
		expect(
			within(articleOf("price-watch")).queryByRole("button", {
				name: "Move a project out",
			}),
		).toBeNull();

		// The estimate is on the button; pressing it asks once and shows the cost.
		fireEvent.click(
			screen.getByRole("button", {
				name: "Suggest names with Claude (about $0.02)",
			}),
		);
		await waitFor(() =>
			expect(h.calls("proposal.names").map((f) => f.params)).toEqual([
				{ confirmed: true },
			]),
		);
		expect(
			await screen.findByText("Names from Claude · cost $0.02"),
		).toBeTruthy();
		const named = within(await waitFor(() => articleOf("Tidepool")));
		expect(
			named.getByText("Surf forecasts: the app, its API and its servers."),
		).toBeTruthy();
		fireEvent.click(
			named.getByRole("button", { name: "Keep the folder name" }),
		);
		await waitFor(() =>
			expect(h.calls("proposal.revert").map((f) => f.params)).toEqual([
				{ id: "c1" },
			]),
		);

		// Create makes them all and goes to All teams, which says so.
		fireEvent.click(screen.getByRole("button", { name: "Create 3 teams" }));
		await waitFor(() =>
			expect(h.calls("proposal.create").map((f) => f.params)).toEqual([
				{ confirmed: true },
			]),
		);
		await waitFor(() => expect(where()).toBe("/teams"));
		expect(created).toBe(true);
		await answerQuery(h, "teams.list", teamList("tidepool"));
		expect(
			await screen.findByText(
				"3 teams created. They're paused until you start them.",
			),
		).toBeTruthy();
	});

	it("offers_the_conversion_for_a_one_repo_team", async () => {
		const { hub, sockets, open } = await renderApp("/find/teams");
		const h = hub as FakeSocket;
		const adding = card({
			id: "c9",
			kind: "add_to_team",
			name: "web",
			team: "shop",
			team_name: "Shop",
			projects: [
				{
					key: "/home/me/Projects/web",
					name: "web",
					folder: "/home/me/Projects/web",
					services: [],
				},
			],
		});
		answerCalls(h, {
			"proposal.get": { cards: [adding], templates: TEMPLATES },
			"proposal.names_estimate": { usd: 0.0213, input_tokens: 3800 },
			"finding.list": { ...NOTHING },
			"proposal.create": {
				made: [],
				added: [],
				failed: [
					{
						id: "c9",
						name: "web",
						reason:
							"conversion_needed: adding web makes shop a team of several repositories",
						team: "shop",
					},
				],
				split: [],
			},
		});

		fireEvent.click(
			await screen.findByRole("button", { name: "Add the projects" }),
		);

		// The team is one repository: the card says what adding makes it, and asks to go on.
		expect(
			await screen.findByText(
				"Adding web makes Shop a team of several repositories.",
			),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Continue" }));

		// Continue opens the team's dialog to add a repository, with the project filled in.
		await waitFor(() => expect(where()).toBe("/t/shop/settings/repositories"));
		await answerQuery(h, "teams.list", teamList("shop"));
		open();
		const team = await waitFor(() => {
			const found = sockets.find(
				(one) => new URL(one.url).pathname === "/t/shop/rpc",
			);
			if (!found) throw new Error("no socket to the team");
			return found;
		});
		open();
		const status = {
			project_root: "/home/me/Projects/shop",
			paused: false,
			signed_in: true,
			port: 7420,
			take_on_error: null,
		};
		await answerQuery(team, "serve.status", status);
		await answerQuery(team, "team.get", {
			team: { name: "Shop", agents: [] },
			agents: [],
			max_agents: 7,
		});
		await answerQuery(team, "repos.list", { repos: null });
		await answerQuery(team, "serve.status", status);
		const dialog = await screen.findByRole("dialog", {
			name: "Add a second repository to shop",
		});
		await answerQuery(team, "project.convert_blockers", {
			tasks: [],
			catervafication: false,
			folder_changes: [],
		});
		await waitFor(() =>
			expect(
				(
					within(dialog).getByRole("button", {
						name: "Continue",
					}) as HTMLButtonElement
				).disabled,
			).toBe(false),
		);
		fireEvent.click(within(dialog).getByRole("button", { name: "Continue" }));
		expect(
			within(dialog).getByText("Second repository: ~/Projects/web"),
		).toBeTruthy();
		expect(
			(
				within(dialog).getByLabelText(
					"Name the second repository",
				) as HTMLInputElement
			).value,
		).toBe("web");
	});
});
