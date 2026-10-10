import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	act,
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { media } from "../test/media.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const WIDE = "(min-width: 1024px)";
const NOW = new Date("2026-10-10T12:00:00Z");
const ago = (days: number) =>
	new Date(NOW.getTime() - days * 86400_000).toISOString();

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	status: "active",
});
const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		agent("ada", "Ada", "architect", "architect"),
		agent("iris", "Iris", "ui_ux_designer", "designer"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};

const ROOT = "docs/catervas/";
const ROADMAP = `${ROOT}product/roadmap.md`;
const OVERVIEW = `${ROOT}architecture/overview.md`;

/** `files.tree` as the daemon answers it: Design owns a folder with nothing in it. */
const tree = (integration = "auto_merge") => ({
	integration,
	folders: [
		{
			folder: `${ROOT}product`,
			name: "Product",
			role: "product_manager",
			agent_id: "mira",
			documents: [
				{
					path: `${ROOT}product/spec.md`,
					name: "Product description",
					twin: `${ROOT}product/spec.agent.md`,
				},
				{
					path: ROADMAP,
					name: "Roadmap",
					twin: `${ROOT}product/roadmap.agent.md`,
				},
			],
		},
		{
			folder: `${ROOT}architecture`,
			name: "Architecture",
			role: "architect",
			agent_id: "ada",
			documents: [
				{ path: OVERVIEW, name: "Overview", twin: null },
				{
					path: `${ROOT}architecture/plans/CTV-12.md`,
					name: "Plan: pie pre-orders",
					twin: null,
				},
			],
		},
		{
			folder: `${ROOT}design`,
			name: "Design",
			role: "ui_ux_designer",
			agent_id: "iris",
			documents: [],
		},
	],
});

type Change = {
	at: string;
	task_id: string | null;
	agent_id: string | null;
	author: string;
	by_you: boolean;
} | null;
const version = (path: string, text: string, last: Change = null) => ({
	path,
	text,
	blob: `blob-of-${text.length}`.padEnd(40, "0"),
	last_change: last,
});
const MIRA_FIVE_DAYS: Change = {
	at: ago(5),
	task_id: "CTV-9",
	agent_id: "mira",
	author: "Mira",
	by_you: false,
};
/** `files.read` as the daemon answers it, the roadmap and its twin unless `read` says otherwise. */
const read = (more: Record<string, unknown> = {}) => ({
	document: version(
		ROADMAP,
		"What your team builds next.\n\n## Now\n\n- Pie pre-orders",
		MIRA_FIVE_DAYS,
	),
	twin: version(
		`${ROOT}product/roadmap.agent.md`,
		"## Now: sprint 5\n\n- CTV-21 Pie pre-orders",
		MIRA_FIVE_DAYS,
	),
	stale: false,
	rederiving: false,
	editable: true,
	waiting: null,
	...more,
});

/** The Files page at `path`, with the team, the tree and the document answered. */
async function files(
	path: string,
	fields: { read?: unknown; tree?: unknown } = {},
) {
	vi.useFakeTimers({ toFake: ["Date"] });
	vi.setSystemTime(NOW);
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "files.tree", fields.tree ?? tree());
	if (fields.read !== null)
		await answerQuery(s, "files.read", fields.read ?? read());
	return { container, s };
}

describe("Files", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("lists each folder with its owner", async () => {
		media.set(WIDE, true);
		await files(`/files/${ROADMAP.slice(ROOT.length)}`);
		const nav = await screen.findByRole("navigation", { name: en.files });
		const product = within(nav).getByTitle("Product Manager");
		expect(product.textContent).toBe("Product");
		const folder = product.closest("details") as HTMLElement;
		expect(within(folder).getByAltText("Mira")).toBeTruthy();
		expect(within(folder).getByText("Mira")).toBeTruthy();
		expect(
			within(nav)
				.getAllByRole("link")
				.map((a) => [a.textContent, a.getAttribute("href")]),
		).toEqual([
			["Product description", "/files/product/spec.md"],
			["Roadmap", "/files/product/roadmap.md"],
			["Overview", "/files/architecture/overview.md"],
			["Plan: pie pre-orders", "/files/architecture/plans/CTV-12.md"],
		]);
		expect(
			within(nav)
				.getByRole("link", { name: "Roadmap" })
				.getAttribute("aria-current"),
		).toBe("page");
		expect(nav.querySelectorAll("details")).toHaveLength(2);
		expect(within(nav).getByText(en.filesNothingHere)).toBeTruthy();
		expect(within(nav).getByText("Iris")).toBeTruthy();
	});

	it("opens a pair on the version for you", async () => {
		media.set(WIDE, true);
		await files("/files/product/roadmap.md");
		await screen.findByRole("heading", { name: "Roadmap" });
		expect(screen.getByText("Product › Roadmap")).toBeTruthy();
		const forYou = screen.getByRole("button", { name: en.filesForYou });
		const forTeam = screen.getByRole("button", { name: en.filesForTeam });
		expect(forYou.getAttribute("aria-pressed")).toBe("true");
		expect(forTeam.getAttribute("aria-pressed")).toBe("false");
		expect(screen.getByText("Pie pre-orders")).toBeTruthy();
		expect(screen.getByRole("button", { name: en.filesEdit })).toBeTruthy();

		fireEvent.click(forTeam);
		expect(screen.getByText("Now: sprint 5")).toBeTruthy();
		expect(
			screen.getByText(
				"This is how your team reads the roadmap. To change it, change the version for you; Mira keeps this one in step.",
			),
		).toBeTruthy();
		expect(screen.queryByRole("button", { name: en.filesEdit })).toBeNull();
		expect(screen.queryByText("Pie pre-orders")).toBeNull();
	});

	it("says who changed it when and in which task", async () => {
		media.set(WIDE, true);
		const line = () =>
			screen.queryByText(
				(_, el) =>
					el?.tagName === "P" &&
					(el.textContent ?? "").startsWith("Changed by"),
			);
		await files("/files/product/roadmap.md");
		await screen.findByRole("heading", { name: "Roadmap" });
		expect(line()?.textContent).toBe("Changed by Mira 5 days ago, in CTV-9");
		expect(
			within(line() as HTMLElement)
				.getByRole("link", { name: "CTV-9" })
				.getAttribute("href"),
		).toBe("/tasks/CTV-9");
		cleanup();

		const you = read({
			document: version(ROADMAP, "Text", {
				at: ago(0),
				task_id: null,
				agent_id: null,
				author: "Sam",
				by_you: true,
			}),
			twin: null,
		});
		await files("/files/product/roadmap.md", { read: you });
		await screen.findByText("Changed by you just now");
		expect(line()?.querySelector("a")).toBeNull();
		cleanup();

		const sam = read({
			document: version(ROADMAP, "Text", {
				at: ago(5),
				task_id: null,
				agent_id: null,
				author: "Sam",
				by_you: false,
			}),
			twin: null,
		});
		await files("/files/product/roadmap.md", { read: sam });
		await screen.findByText("Changed by Sam 5 days ago");
		expect(line()?.querySelector("a")).toBeNull();
		cleanup();

		await files("/files/product/roadmap.md", {
			read: read({ document: version(ROADMAP, "Text", null), twin: null }),
		});
		await screen.findByRole("heading", { name: "Roadmap" });
		expect(line()).toBeNull();
	});

	it("says when the team's version is being brought up to date", async () => {
		media.set(WIDE, true);
		await files("/files/product/roadmap.md", {
			read: read({ stale: true, rederiving: true }),
		});
		await screen.findByRole("heading", { name: "Roadmap" });
		expect(
			screen.queryByText(en.filesBringing.replace("{name}", "Mira")),
		).toBeNull();
		fireEvent.click(screen.getByRole("button", { name: en.filesForTeam }));
		const notice = screen
			.getByText("Mira is bringing this up to date with your change.")
			.closest("p") as HTMLElement;
		expect(within(notice).getByAltText("Mira")).toBeTruthy();
		cleanup();

		await files("/files/product/roadmap.md", {
			read: read({ stale: true, rederiving: false }),
		});
		await screen.findByRole("heading", { name: "Roadmap" });
		fireEvent.click(screen.getByRole("button", { name: en.filesForTeam }));
		const older = screen.getByText(en.filesOlder);
		expect(older.textContent).not.toContain("Mira");
		expect(screen.queryByText(/bringing this up to date/)).toBeNull();
	});

	it("says a one-file document is written for the team", async () => {
		media.set(WIDE, true);
		await files(`/files/${OVERVIEW.slice(ROOT.length)}`, {
			read: read({
				document: version(OVERVIEW, "Text", MIRA_FIVE_DAYS),
				twin: null,
			}),
		});
		await screen.findByRole("heading", { name: "Overview" });
		expect(screen.queryByText(en.filesWhichVersion)).toBeNull();
		expect(
			screen.queryByRole("group", { name: en.filesWhichVersion }),
		).toBeNull();
		expect(screen.getByText(en.filesWrittenForTeam)).toBeTruthy();
		expect(screen.getByRole("button", { name: en.filesEdit })).toBeTruthy();
		cleanup();

		await files(`/files/${OVERVIEW.slice(ROOT.length)}`, {
			read: read({
				document: version(OVERVIEW, "Text", MIRA_FIVE_DAYS),
				twin: null,
				editable: false,
			}),
		});
		await screen.findByRole("heading", { name: "Overview" });
		expect(screen.queryByRole("button", { name: en.filesEdit })).toBeNull();
	});

	it("opens the first document at files", async () => {
		media.set(WIDE, true);
		const { s } = await files("/files", {
			read: read({ document: version(`${ROOT}product/spec.md`, "Spec", null) }),
		});
		const asked = s
			.calls("query")
			.map((q) => (q.params as { params: { path?: string } }).params.path);
		expect(asked).toContain(`${ROOT}product/spec.md`);
		await screen.findByRole("heading", { name: "Product description" });
		expect(screen.getByText("Product › Product description")).toBeTruthy();
	});

	it("says there is no such document", async () => {
		media.set(WIDE, true);
		await files("/files/product/nothing.md", { read: null });
		expect(await screen.findByText(en.filesNoSuch)).toBeTruthy();
	});

	it("picks a document on a phone", async () => {
		media.set(WIDE, false);
		const { s } = await files("/files/product/roadmap.md");
		await screen.findByRole("heading", { name: "Roadmap" });
		expect(
			screen.getByRole("link", { name: en.filesBack }).getAttribute("href"),
		).toBe("/team");
		const pick = screen.getByRole("combobox", { name: en.filesFile });
		expect([...pick.querySelectorAll("optgroup")].map((g) => g.label)).toEqual([
			"Product, Mira",
			"Architecture, Ada",
			"Design, Iris",
		]);
		const none = within(pick).getByRole("option", {
			name: "Design: nothing here yet",
		}) as HTMLOptionElement;
		expect(none.disabled).toBe(true);
		expect(screen.getByText("Mira’s folder, Product")).toBeTruthy();
		fireEvent.change(pick, { target: { value: OVERVIEW } });
		await waitFor(() =>
			expect(
				s
					.calls("query")
					.map((q) => (q.params as { params: { path?: string } }).params.path),
			).toContain(OVERVIEW),
		);
	});

	it("has no accessibility violations", async () => {
		media.set(WIDE, true);
		const wide = await files("/files/product/roadmap.md");
		await screen.findByRole("heading", { name: "Roadmap" });
		await expectNoAxeViolations(wide.container);
		cleanup();
		act(() => media.set(WIDE, false));
		const narrow = await files("/files/product/roadmap.md");
		await screen.findByRole("heading", { name: "Roadmap" });
		await expectNoAxeViolations(narrow.container);
	});
});
