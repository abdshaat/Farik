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
import {
	answerQuery,
	answerStatus,
	eventArrives,
	renderApp,
} from "../test/render-app.tsx";

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
	askable: true,
	waiting: null,
	...more,
});

/** The Files page at `path`, with the team, the tree and the document answered. */
async function files(
	path: string,
	fields: { read?: unknown; tree?: unknown; catervafication?: unknown } = {},
) {
	vi.useFakeTimers({ toFake: ["Date"] });
	vi.setSystemTime(NOW);
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "files.tree", fields.tree ?? tree());
	if (fields.catervafication)
		await answerQuery(s, "catervafication.get", fields.catervafication);
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

	it("shows_the_same_start_on_the_files_page", async () => {
		media.set(WIDE, true);
		await files("/files/product/roadmap.md", {
			catervafication: { offered: true, roles: ["product_manager"] },
		});
		const title = await screen.findByText(
			"Your team hasn\u2019t documented this project yet",
		);
		const start = screen.getByRole("button", { name: "Start" });
		expect(start.getAttribute("aria-describedby")).toBe(title.id);
		// It heads the page: before the folders and the document.
		const nav = screen.getByRole("navigation", { name: en.files });
		expect(
			title.compareDocumentPosition(nav) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		cleanup();

		await files("/files/product/roadmap.md", {
			catervafication: { offered: false, roles: [] },
		});
		await screen.findByRole("heading", { name: "Roadmap" });
		expect(screen.queryByText(/documented this project/)).toBeNull();
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
			screen.queryByText(en.filesBringing.replace("{owner}", "Mira")),
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

	it("shows a document's title once", async () => {
		media.set(WIDE, true);
		await files("/files/product/roadmap.md", {
			read: read({
				document: version(ROADMAP, "# Roadmap\n\nPie pre-orders", null),
				twin: null,
			}),
		});
		await screen.findByText("Pie pre-orders");
		expect(screen.getAllByRole("heading", { name: "Roadmap" })).toHaveLength(1);
		cleanup();

		// A first heading of its own is the title, as the document words it.
		await files("/files/product/roadmap.md", {
			read: read({
				document: version(ROADMAP, "# Where we go ##\n\nPie", null),
				twin: null,
			}),
		});
		await screen.findByText("Pie");
		expect(
			screen.getAllByRole("heading", { name: "Where we go" }),
		).toHaveLength(1);
		cleanup();

		// With no heading of its own, the plain name is the title.
		await files("/files/product/roadmap.md", { read: read({ twin: null }) });
		expect(
			(await screen.findAllByRole("heading", { name: "Roadmap" })).length,
		).toBe(1);
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
				askable: false,
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

	describe("editing", () => {
		const draftText = "# New heading\n\nWords of mine.";
		const SAVED = { change: 1, branch: "docs/folder-1" };
		const box = () => screen.getByRole("textbox", { name: "Roadmap text" });
		const queries = (s: FakeSocket, name: string) =>
			s.calls("query").filter((q) => q.params.name === name);
		const press = (name: string) =>
			fireEvent.click(screen.getByRole("button", { name }));

		/** The roadmap's editor open, with `draftText` typed into it. */
		async function typed(fields: { read?: unknown; tree?: unknown } = {}) {
			media.set(WIDE, true);
			const opened = await files("/files/product/roadmap.md", fields);
			await screen.findByRole("heading", { name: "Roadmap" });
			press(en.filesEdit);
			fireEvent.change(box(), { target: { value: draftText } });
			return opened;
		}

		it("edits and saves the version for you", async () => {
			media.set(WIDE, true);
			const { s } = await files("/files/product/roadmap.md");
			await screen.findByRole("heading", { name: "Roadmap" });
			press(en.filesEdit);
			expect(screen.getByText(en.filesEditing)).toBeTruthy();
			expect(
				screen
					.getByRole("button", { name: en.filesWrite })
					.getAttribute("aria-pressed"),
			).toBe("true");
			expect(screen.getByText(en.filesHint)).toBeTruthy();
			expect((box() as HTMLTextAreaElement).value).toBe(read().document.text);
			expect(
				screen.getByText(
					"Saving keeps your change at once. Mira will update the team’s version.",
				),
			).toBeTruthy();

			fireEvent.change(box(), { target: { value: draftText } });
			press(en.filesPreview);
			expect(screen.getByRole("heading", { name: "New heading" })).toBeTruthy();
			expect(screen.queryByRole("textbox")).toBeNull();

			const before = [
				queries(s, "files.read").length,
				queries(s, "files.tree").length,
			];
			press(en.filesSave);
			const [frame, ...more] = s.calls("files.save");
			expect(more).toEqual([]);
			expect(frame?.params).toEqual({
				path: ROADMAP,
				text: draftText,
				base: read().document.blob,
			});
			await s.reply(frame as never, SAVED);
			await waitFor(() =>
				expect(screen.queryByText(en.filesEditing)).toBeNull(),
			);
			expect(screen.getByRole("button", { name: en.filesEdit })).toBeTruthy();
			expect(queries(s, "files.read").length).toBeGreaterThan(before[0] ?? 0);
			expect(queries(s, "files.tree").length).toBeGreaterThan(before[1] ?? 0);
		});

		it("says what saving does under each policy", async () => {
			for (const [integration, words] of [
				[
					"pull_request",
					"Saving puts your change in a pull request; it is in your project once that is merged. Then Mira updates the team’s version.",
				],
				[
					"manual",
					"Saving puts your change on a branch for you to add with catervas integrate. Then Mira updates the team’s version.",
				],
			] as const) {
				media.set(WIDE, true);
				await files("/files/product/roadmap.md", { tree: tree(integration) });
				await screen.findByRole("heading", { name: "Roadmap" });
				press(en.filesEdit);
				expect(screen.getByText(words)).toBeTruthy();
				expect(document.body.textContent).not.toContain("at once");
				cleanup();
			}
		});

		it("says a saved change is on its way", async () => {
			for (const [integration, words] of [
				["auto_merge", "Saved. Catervas adds it to your project in a moment."],
				["pull_request", "Saved. It goes out with a pull request."],
				[
					"manual",
					"Saved. It goes out when you run catervas integrate folder-3.",
				],
			] as const) {
				media.set(WIDE, true);
				const { s } = await files("/files/product/roadmap.md", {
					tree: tree(integration),
					read: read({ waiting: { change: 3, branch: "docs/folder-3" } }),
				});
				await screen.findByText(words);
				expect(screen.queryByRole("button", { name: en.filesEdit })).toBeNull();
				expect(document.body.textContent).not.toContain("at once");
				if (integration === "auto_merge") {
					await eventArrives(s, 9);
					await answerQuery(s, "files.read", read());
					expect(
						await screen.findByRole("button", { name: en.filesEdit }),
					).toBeTruthy();
					expect(screen.queryByText(words)).toBeNull();
				}
				cleanup();
			}
		});

		it("keeps the draft while events arrive", async () => {
			const { s } = await typed();
			await eventArrives(s, 9);
			await answerQuery(
				s,
				"files.read",
				read({ document: version(ROADMAP, "Changed meanwhile", null) }),
			);
			await screen.findByRole("textbox", { name: "Roadmap text" });
			expect((box() as HTMLTextAreaElement).value).toBe(draftText);
			press(en.filesSave);
			expect(s.calls("files.save")[0]?.params).toMatchObject({
				text: draftText,
				base: read().document.blob,
			});
		});

		it("keeps the owner's text when the file changed", async () => {
			const { s } = await typed();
			press(en.filesSave);
			await s.fail(s.calls("files.save")[0] as never, -32005, "refused", {
				errors: [{ path: "/base", code: "file_changed", message: "changed" }],
			});
			const alert = await screen.findByRole("alert");
			expect(alert.textContent).toContain(en.filesChangedWhile);
			expect((box() as HTMLTextAreaElement).value).toBe(draftText);

			const asked = queries(s, "files.read").length;
			fireEvent.click(
				within(alert).getByRole("button", { name: en.filesReload }),
			);
			await waitFor(() =>
				expect(queries(s, "files.read").length).toBeGreaterThan(asked),
			);
			const fresh = read({
				document: version(ROADMAP, "Changed on the other side", null),
			});
			await answerQuery(s, "files.read", fresh);
			await waitFor(() =>
				expect((box() as HTMLTextAreaElement).value).toBe(
					"Changed on the other side",
				),
			);
			expect(screen.queryByRole("alert")).toBeNull();
			press(en.filesSave);
			expect(s.calls("files.save")[1]?.params).toMatchObject({
				base: fresh.document.blob,
			});
		});

		it("says any other refusal and keeps the draft", async () => {
			const { s } = await typed();
			let sent = 0;
			for (const [path, code, words] of [
				[
					"/path",
					"change_waiting",
					"An earlier change to this file is not in your project yet. Edit it once it is.",
				],
				[
					"/path",
					"file_busy",
					"This file was also changed on your computer, outside Catervas. Finish or undo that change, then save here again.",
				],
				[
					"/path",
					"marketing_plan",
					"A marketing plan changes through a new version on its page.",
				],
				[
					"/path",
					"is_a_link",
					"This file is a link, which Catervas does not write through.",
				],
				[
					"/text",
					"too_large",
					"This is too long to save here: a document can be at most about 256 KB.",
				],
			] as const) {
				press(en.filesSave);
				await s.fail(
					s.calls("files.save")[sent++] as never,
					-32005,
					"refused",
					{
						errors: [{ path, code, message: "raw words" }],
					},
				);
				expect((await screen.findByRole("alert")).textContent).toBe(words);
				expect((box() as HTMLTextAreaElement).value).toBe(draftText);
			}
		});

		it("keeps the draft when the screen changes size", async () => {
			await typed();
			act(() => media.set(WIDE, false));
			await screen.findByRole("link", { name: en.filesBack });
			expect((box() as HTMLTextAreaElement).value).toBe(draftText);
			act(() => media.set(WIDE, true));
			await screen.findByRole("navigation", { name: en.files });
			expect((box() as HTMLTextAreaElement).value).toBe(draftText);
		});

		it("cancels without saving", async () => {
			const { s } = await typed();
			press(en.filesCancel);
			expect(screen.queryByRole("textbox")).toBeNull();
			expect(screen.getByText("Pie pre-orders")).toBeTruthy();
			expect(s.calls("files.save")).toEqual([]);
		});

		it("offers no edit of the team's version", async () => {
			media.set(WIDE, true);
			await files("/files/product/roadmap.md");
			await screen.findByRole("heading", { name: "Roadmap" });
			press(en.filesForTeam);
			expect(screen.queryByRole("button", { name: en.filesEdit })).toBeNull();
			cleanup();

			await files(`/files/${OVERVIEW.slice(ROOT.length)}`, {
				read: read({
					document: version(OVERVIEW, "Text", MIRA_FIVE_DAYS),
					twin: null,
				}),
			});
			await screen.findByRole("heading", { name: "Overview" });
			press(en.filesEdit);
			expect(screen.queryByText(en.filesEditing)).toBeNull();
			// The whole footnote is the policy's sentence: no owner follows it.
			expect(screen.getByText(en.filesSaveAuto)).toBeTruthy();
		});
	});
});
