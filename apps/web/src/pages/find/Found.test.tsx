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

const ago = (days: number) =>
	new Date(Date.now() - days * 86_400_000).toISOString();

const LIST = {
	main: [
		row({
			key: "github.com/me/tidepool-web",
			name: "tidepool-web",
			folders: ["/home/me/code/tidepool-web", "/home/me/work/tidepool-web"],
			last_at: ago(0),
			services: ["GitHub", "Vercel"],
		}),
		row({
			key: "/home/me/code/tidepool-api",
			name: "tidepool-api",
			folders: ["/home/me/code/tidepool-api"],
			last_at: ago(1),
			jobs: [{ name: "Nightly cleanup", schedule: "every night at 02:00" }],
			services: ["Stripe"],
		}),
		row({
			key: "/home/me/code/price-watch",
			name: "price-watch",
			folders: ["/home/me/code/price-watch"],
			last_at: ago(47),
			suggested: "watch",
		}),
		row({
			key: "/home/me/code/tidepool",
			name: "tidepool",
			folders: ["/home/me/code/tidepool"],
			last_at: ago(2),
			already_team: "tidepool",
		}),
		row({
			key: "Bookkeeping",
			name: "Bookkeeping",
			kind: "connector",
			folders: [],
			last_at: ago(5),
			services: ["Zoho Books", "Excel"],
		}),
	],
	explored: [
		row({
			key: "/home/me/code/awesome-llm-tools",
			name: "awesome-llm-tools",
			last_at: ago(12),
			suggested: "ignore",
		}),
	],
	gone: [
		row({
			key: "/home/me/code/old-landing",
			name: "old-landing",
			folders: ["/home/me/code/old-landing"],
			last_at: ago(26),
			suggested: null,
			gone: true,
		}),
	],
	home: { sessions: 52, formed_into: ["Bookkeeping"] },
	computer_jobs: [
		row({
			key: "job:crontab:/etc/crontab:nightly-backup",
			name: "nightly-backup",
			kind: "job",
			folders: [],
			jobs: [{ name: "nightly-backup", schedule: "Every night at 02:00" }],
			suggested: "watch",
		}),
	],
	ignored: [],
};

/** The team list with Tidepool in it, which the already-a-team row names. */
const TEAMS = {
	...teamList("tidepool"),
	teams: [{ ...teamList("tidepool").teams[0], name: "Tidepool" }],
};

const rowOf = (name: string) =>
	screen.getByText(name, { selector: "strong" }).closest("li") as HTMLElement;

describe("here's what you work on", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
		localStorage.clear();
	});

	it("lists_what_you_work_on_with_a_choice_each", async () => {
		const { hub } = await renderApp("/find/found");
		const h = hub as FakeSocket;
		answerCalls(h, {
			"finding.status": { ...IDLE, phase: "done", index_exists: true },
			"finding.list": LIST,
			"finding.choose": {},
			"finding.separate": {},
			"finding.locate": {},
		});
		await answerQuery(h, "teams.list", TEAMS);

		// One row each: the name, its jobs, last worked and services, with the suggested choice.
		expect(await screen.findByText("Here's what you work on")).toBeTruthy();
		await screen.findByText("tidepool-web", { selector: "strong" });
		const web = within(rowOf("tidepool-web"));
		expect(web.getByText("Today · GitHub, Vercel")).toBeTruthy();
		expect(
			(web.getByRole("radio", { name: "Team" }) as HTMLInputElement).checked,
		).toBe(true);
		const api = within(rowOf("tidepool-api"));
		expect(api.getByText("1 job")).toBeTruthy();
		expect(api.getByText("Yesterday · Stripe")).toBeTruthy();
		const price = within(rowOf("price-watch"));
		expect(
			(price.getByRole("radio", { name: "Watch" }) as HTMLInputElement).checked,
		).toBe(true);
		expect(price.getByText("47 days ago")).toBeTruthy();

		// A choice goes to the hub.
		fireEvent.click(api.getByRole("radio", { name: "Watch" }));
		const chose = await waitFor(() => {
			const frame = h.calls("finding.choose")[0];
			if (!frame) throw new Error("no finding.choose was sent");
			return frame;
		});
		expect(chose.params).toEqual({
			key: "/home/me/code/tidepool-api",
			choice: "watch",
		});
		expect(
			(api.getByRole("radio", { name: "Watch" }) as HTMLInputElement).checked,
		).toBe(true);

		// Two folders of one repository: they can be told apart.
		expect(web.getByText("2 folders")).toBeTruthy();
		fireEvent.click(web.getByRole("button", { name: "More about 2 folders" }));
		expect(
			web.getByText(
				"~/code/tidepool-web and ~/work/tidepool-web are the same GitHub repository.",
			),
		).toBeTruthy();
		fireEvent.click(web.getByRole("button", { name: "These are different" }));
		const separate = await waitFor(() => {
			const frame = h.calls("finding.separate")[0];
			if (!frame) throw new Error("no finding.separate was sent");
			return frame;
		});
		expect(separate.params).toEqual({
			key: "github.com/me/tidepool-web",
			folder: "/home/me/work/tidepool-web",
		});

		// A project found through a connector says so, and where its team would live.
		const books = within(rowOf("Bookkeeping"));
		fireEvent.click(
			books.getByRole("button", { name: "More about Bookkeeping" }),
		);
		expect(
			books.getByText("Found from Claude's connectors, not a folder."),
		).toBeTruthy();
		expect(
			books.getByText(
				"A team for it gets a new folder: ~/Catervas/bookkeeping.",
			),
		).toBeTruthy();

		// A team that already drives a project shows it, with no choice.
		const team = within(rowOf("tidepool"));
		expect(team.getByText("Already a team · Tidepool")).toBeTruthy();
		expect(team.queryByRole("radio")).toBeNull();

		// A folder that is gone is listed, not offered: Locate… or Not mine.
		fireEvent.click(
			screen.getByRole("button", { name: "No longer on this computer (1)" }),
		);
		const gone = within(rowOf("old-landing"));
		expect(gone.getByRole("button", { name: "Locate…" })).toBeTruthy();
		fireEvent.click(gone.getByRole("button", { name: "Not mine" }));
		await waitFor(() => expect(h.calls("finding.choose")).toHaveLength(2));
		expect(h.calls("finding.choose")[1]?.params).toEqual({
			key: "/home/me/code/old-landing",
			choice: "ignore",
		});

		// The tucked-away sections open to their rows.
		fireEvent.click(screen.getByRole("button", { name: "Explored (1)" }));
		expect(
			within(rowOf("awesome-llm-tools")).getByRole("radio", { name: "Ignore" }),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Your home folder" }));
		expect(screen.getByText("52 sessions here.")).toBeTruthy();
		fireEvent.click(
			screen.getByRole("button", { name: "Jobs on this computer (1)" }),
		);
		const job = within(rowOf("nightly-backup"));
		expect(job.getByText("Every night at 02:00")).toBeTruthy();
		expect(job.queryByRole("radio", { name: "Team" })).toBeNull();

		// Continue leads on to "Do these work together?".
		fireEvent.click(screen.getByRole("button", { name: "Continue" }));
		await waitFor(() => expect(where()).toBe("/find/together"));
	});

	it("offers_a_project_by_hand_when_nothing_is_found", async () => {
		const { hub } = await renderApp("/find/found");
		const h = hub as FakeSocket;
		answerCalls(h, {
			"finding.status": { ...IDLE, phase: "done", index_exists: true },
			"finding.list": NOTHING,
			"finding.add_project": {
				name: "Corner shop",
				root: null,
				repository: "github.com/me/corner-shop",
				same_as: null,
			},
		});
		expect(await screen.findByText("We didn't find any projects")).toBeTruthy();
		expect(
			screen.getByText("Your history may be in another place."),
		).toBeTruthy();
		fireEvent.click(
			screen.getByRole("button", { name: "Add a project by hand" }),
		);
		const dialog = await screen.findByRole("dialog");
		fireEvent.change(within(dialog).getByLabelText("Name"), {
			target: { value: "Corner shop" },
		});
		fireEvent.change(within(dialog).getByLabelText("Repository address"), {
			target: { value: "github.com/me/corner-shop" },
		});
		fireEvent.click(within(dialog).getByRole("button", { name: "Add" }));
		const added = await waitFor(() => {
			const frame = h.calls("finding.add_project")[0];
			if (!frame) throw new Error("no finding.add_project was sent");
			return frame;
		});
		expect(added.params).toEqual({
			name: "Corner shop",
			repository: "github.com/me/corner-shop",
		});
	});
});
