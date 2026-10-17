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
import { useConnection } from "../../app/connection.tsx";
import { en } from "../../strings/en.ts";
import type { FakeSocket } from "../../test/fake-socket.ts";
import {
	answerQuery,
	answerStatus,
	renderApp,
} from "../../test/render-app.tsx";
import { bodyOf, refusedBy } from "../../test/schema.ts";
import { joinedNames } from "./SetupReady.tsx";

const READY = { state: "ready", version: "2.1.300" };
const MISSING = { state: "missing" };
const NO_PROJECT = { project_root: null, credential: null };

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	status: "active",
});
/** The mockup's team: Mira, Sol, Ada, Theo and Kai. */
const TEAM = {
	name: "Corner Bakery orders",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		agent("sol", "Sol", "scrum_master", "scrum-master"),
		agent("ada", "Ada", "architect", "architect"),
		agent("theo", "Theo", "software_developer", "developer"),
		agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};
const OFFER = {
	offered: true,
	roles: [
		"product_manager",
		"architect",
		"software_developer",
		"marketing_specialist",
	],
};

/** The offer page at the end of setup, with its two queries answered. */
async function theOffer(offer: unknown = OFFER, team: unknown = TEAM) {
	const rendered = await renderApp("/setup/ready");
	const s = rendered.socket as FakeSocket;
	await answerQuery(s, "catervafication.get", offer);
	await answerQuery(s, "team.get", { team });
	return { ...rendered, s };
}

function Status() {
	return <p data-testid="status">{useConnection().status}</p>;
}

/** The latest request of `method` the page sent. */
async function sent(socket: FakeSocket, method: string) {
	return waitFor(() => {
		const frame = socket.calls(method).at(-1);
		if (!frame) throw new Error(`no ${method} was sent`);
		return frame;
	});
}

describe("setup", () => {
	afterEach(() => {
		vi.unstubAllGlobals();
		sessionStorage.clear();
	});

	it("checks_the_computer_and_offers_the_fixes", async () => {
		const first = await renderApp("/setup/computer");
		const socket = first.socket as FakeSocket;
		await answerQuery(socket, "computer.check", {
			claude: READY,
			git: { state: "ready", version: "2.47.0" },
			docker: MISSING,
			sandbox_image: MISSING,
		});
		const docker = (await screen.findByText(en.computerDocker)).closest("li");
		if (!docker) throw new Error("no Docker row");
		expect(within(docker).getByText(en.notFound)).toBeTruthy();
		expect(within(docker).getByText(en.dockerMissing)).toBeTruthy();
		// Only the ready word wears the pale green pill.
		const claude = (await screen.findByText(en.computerClaude)).closest("li");
		if (!claude) throw new Error("no Claude row");
		expect(within(claude).getByText(en.ready).className).toMatch(/pill/);
		expect(within(docker).getByText(en.notFound).className).not.toMatch(/pill/);
		// The warning names what the command line's does, the connectors' keys among it (8.6).
		expect(
			screen.getByText(
				"Without Docker, a mistaken or tricked agent command can reach any file you can, and the keys you gave your agents’ connectors. Catervas reminds you of this every time it starts.",
			),
		).toBeTruthy();
		const onward = screen.getByRole("button", { name: en.continue });
		expect((onward as HTMLButtonElement).disabled).toBe(true);
		await expectNoAxeViolations(first.container);

		fireEvent.click(
			screen.getByRole("button", { name: en.continueWithoutDocker }),
		);
		expect(
			await screen.findByRole("heading", { name: en.accountTitle }),
		).toBeTruthy();
		expect(sessionStorage.getItem("catervas.noSandbox")).toBe("true");
		cleanup();

		const second = await renderApp("/setup/computer");
		const again = second.socket as FakeSocket;
		await answerQuery(again, "computer.check", {
			claude: READY,
			git: READY,
			docker: READY,
			sandbox_image: MISSING,
		});
		const prepare = await screen.findByRole("button", { name: en.prepare });
		fireEvent.click(prepare);
		const build = await sent(again, "sandbox.build");
		expect(build.params).toEqual({});
		expect(
			screen
				.getByRole("button", { name: new RegExp(en.prepare) })
				.getAttribute("aria-busy"),
		).toBe("true");
		await again.reply(build, { image: "catervas/sandbox:1" });
		// The image is looked for again once it is built.
		await waitFor(() =>
			expect(
				again.calls("query").filter((f) => f.params.name === "computer.check"),
			).toHaveLength(2),
		);
	});

	it("connects_a_subscription_key", async () => {
		const { container, socket } = await renderApp("/setup/account");
		const s = socket as FakeSocket;
		await answerQuery(s, "account.status", {
			provider: null,
			kind: null,
			source: null,
		});
		const field = await screen.findByLabelText(en.subscriptionKey);
		await expectNoAxeViolations(container);

		fireEvent.change(field, { target: { value: "sk-ant-api01-wrong" } });
		fireEvent.click(screen.getByRole("button", { name: en.saveContinue }));
		const refused = await sent(s, "account.connect");
		await s.fail(
			refused,
			-32005,
			"that is not a Claude subscription token: a subscription token starts with sk-ant-oat",
		);
		const why = await screen.findByText(en.setupNotSubscription);
		expect(field.getAttribute("aria-describedby")).toContain(why.id);

		fireEvent.change(field, { target: { value: "sk-ant-oat01-test" } });
		fireEvent.click(screen.getByRole("button", { name: en.saveContinue }));
		await waitFor(() => expect(s.calls("account.connect")).toHaveLength(2));
		const call = s.calls("account.connect")[1];
		if (!call) throw new Error("no second account.connect");
		expect(call.params).toEqual({
			kind: "subscription_token",
			secret: "sk-ant-oat01-test",
		});
		await s.reply(call, { stored_in: "file", taking_on: false });
		expect(await screen.findByText(en.storedFile)).toBeTruthy();
		expect(
			await screen.findByRole("heading", { name: en.projectTitle }),
		).toBeTruthy();
		cleanup();

		// A computer with a keychain: the page says the key went there.
		const again = (await renderApp("/setup/account")).socket as FakeSocket;
		await answerQuery(again, "account.status", {
			provider: null,
			kind: null,
			source: null,
		});
		fireEvent.change(await screen.findByLabelText(en.subscriptionKey), {
			target: { value: "sk-ant-oat01-test" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.saveContinue }));
		const kept = await sent(again, "account.connect");
		await again.reply(kept, { stored_in: "keychain", taking_on: false });
		expect(await screen.findByText(en.storedKeychain)).toBeTruthy();
		expect(screen.queryByText(en.storedFile)).toBeNull();
	});

	it("says_each_setup_refusal_in_plain_words", async () => {
		// The folder browser: a folder it cannot read, said without the system's words.
		sessionStorage.setItem("catervas.noSandbox", "true");
		const project = await renderApp("/setup/project");
		const s = project.socket as FakeSocket;
		await answerStatus(s, false, 1, NO_PROJECT);
		fireEvent.click(await screen.findByRole("button", { name: en.continue }));
		const listed = await waitFor(() => {
			const frame = s
				.calls("query")
				.find((f) => f.params.name === "folders.list");
			if (!frame) throw new Error("no folders.list was sent");
			return frame;
		});
		await s.fail(
			listed,
			-32005,
			"that folder cannot be read: Permission denied (os error 13)",
		);
		expect(await screen.findByText(en.setupUnreadable)).toBeTruthy();
		expect(screen.queryByText(/os error/)).toBeNull();
		cleanup();

		// A new project Catervas could not make, for a reason it has no words for: a plain sentence.
		const fresh = await renderApp("/setup/project");
		const n = fresh.socket as FakeSocket;
		await answerStatus(n, false, 1, NO_PROJECT);
		fireEvent.click(await screen.findByRole("radio", { name: /^No, start/ }));
		fireEvent.change(screen.getByLabelText(en.newWhat), {
			target: {
				value: "An ordering site for my bakery, where customers pick up.",
			},
		});
		fireEvent.change(screen.getByLabelText(en.newName), {
			target: { value: "corner-bakery" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.continue }));
		const create = await sent(n, "project.create");
		await n.fail(
			create,
			-32603,
			"README.md cannot be written: No space left on device (os error 28)",
		);
		expect(await screen.findByText(en.setupRefused)).toBeTruthy();
		expect(screen.queryByText(/os error/)).toBeNull();
		cleanup();

		// The computer: a sandbox that would not build, and a browser with no Docker to fetch it.
		const computer = await renderApp("/setup/computer");
		const c = computer.socket as FakeSocket;
		await answerQuery(c, "computer.check", {
			claude: READY,
			git: READY,
			docker: READY,
			sandbox_image: MISSING,
			designer_browser: MISSING,
		});
		fireEvent.click(await screen.findByRole("button", { name: en.prepare }));
		await c.fail(
			await sent(c, "sandbox.build"),
			-32005,
			"the sandbox image could not be built: step 3/7 exited 1",
		);
		expect(await screen.findByText(en.setupBuildFailed)).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: en.fetchIt }));
		await c.fail(
			await sent(c, "browser.pull"),
			-32005,
			"Docker is not installed",
		);
		expect(await screen.findByText(en.setupNoDocker)).toBeTruthy();
		expect(screen.queryByText(/step 3/)).toBeNull();
	});

	it("takes_the_waiting_project_on_once_connected", async () => {
		const { socket, sockets } = await renderApp(
			"/setup/account",
			{ "GET /session": 204 },
			<Status />,
		);
		const s = socket as FakeSocket;
		await answerQuery(s, "account.status", {
			provider: null,
			kind: null,
			source: null,
		});
		const field = await screen.findByLabelText(en.subscriptionKey);
		fireEvent.change(field, { target: { value: "sk-ant-oat01-test" } });
		fireEvent.click(screen.getByRole("button", { name: en.saveContinue }));
		const call = await sent(s, "account.connect");
		await s.reply(call, { stored_in: "file", taking_on: true });
		expect(await screen.findByText(en.opening)).toBeTruthy();
		expect(screen.getByTestId("status").textContent).toBe("reopening");

		// The setup daemon stops: the page asks for its session again at once, then goes home.
		act(() => s.close());
		expect(screen.queryByText(en.lostTitle)).toBeNull();
		await waitFor(() => expect(sockets).toHaveLength(2));
		const next = sockets[1] as FakeSocket;
		act(() => next.emit("open", {}));
		await answerStatus(next, true);
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
	});

	it("browses_folders_and_uses_one", async () => {
		sessionStorage.setItem("catervas.noSandbox", "true");
		const { container, socket } = await renderApp("/setup/project");
		const s = socket as FakeSocket;
		await answerStatus(s, false, 1, NO_PROJECT);
		fireEvent.click(await screen.findByRole("button", { name: en.continue }));
		await answerQuery(s, "folders.list", {
			path: "",
			parent: null,
			entries: [
				{ name: "notes", git: false },
				{ name: "Projects", git: false },
			],
		});
		const folders = await screen.findByRole("group", { name: en.folders });
		expect(within(folders).getAllByText(en.notGitProject)).toHaveLength(2);
		await expectNoAxeViolations(container);

		fireEvent.click(within(folders).getByRole("button", { name: /Projects/ }));
		fireEvent.click(screen.getByRole("button", { name: en.openFolder }));
		await waitFor(() =>
			expect(
				s
					.calls("query")
					.some(
						(f) =>
							f.params.name === "folders.list" &&
							(f.params.params as { path?: string }).path === "Projects",
					),
			).toBe(true),
		);
		await answerQuery(s, "folders.list", {
			path: "Projects",
			parent: "",
			entries: [
				{ name: "corner-bakery", git: true },
				{ name: "photos-2025", git: false },
			],
		});
		const bakery = await within(folders).findByRole("button", {
			name: /corner-bakery/,
		});
		expect(within(bakery).getByText(en.gitProject)).toBeTruthy();
		// A folder that is not a git project cannot be used.
		fireEvent.click(
			within(folders).getByRole("button", { name: /photos-2025/ }),
		);
		const use = screen.getByRole("button", { name: en.useFolder });
		expect((use as HTMLButtonElement).disabled).toBe(true);
		fireEvent.click(bakery);
		expect((use as HTMLButtonElement).disabled).toBe(false);
		fireEvent.click(screen.getByRole("button", { name: en.useFolder }));
		const open = await sent(s, "project.open");
		expect(open.params).toEqual({
			path: "Projects/corner-bakery",
			no_sandbox: true,
		});
	});

	it("moving_the_team_offers_to_stay", async () => {
		const { socket } = await renderApp("/setup/project");
		const s = socket as FakeSocket;
		await answerStatus(s, false, 1, {
			project_root: null,
			leaving: "/h/old-repo",
		});
		expect(
			await screen.findByText("Moving your team from old-repo"),
		).toBeTruthy();
		expect(screen.queryByRole("button", { name: en.back })).toBeNull();
		expect(
			within(
				screen.getByRole("navigation", { name: en.setupSteps }),
			).queryAllByRole("button"),
		).toHaveLength(0);
		fireEvent.click(screen.getByRole("button", { name: "Stay on old-repo" }));
		const open = await sent(s, "project.open");
		expect(open.params).toEqual({ path: "/h/old-repo", no_sandbox: false });
		expect(refusedBy("projectOpenRequest", open.params)).toEqual([]);
	});

	it("asks_before_replacing_a_team", async () => {
		sessionStorage.setItem("catervas.noSandbox", "true");
		const { socket } = await renderApp("/setup/project");
		const s = socket as FakeSocket;
		await answerStatus(s, false, 1, {
			project_root: null,
			leaving: "/h/old-repo",
		});
		fireEvent.click(await screen.findByRole("button", { name: en.continue }));
		await answerQuery(s, "folders.list", {
			path: "",
			parent: null,
			entries: [{ name: "new-repo", git: true }],
		});
		const folders = await screen.findByRole("group", { name: en.folders });
		fireEvent.click(within(folders).getByRole("button", { name: /new-repo/ }));
		fireEvent.click(screen.getByRole("button", { name: en.useFolder }));
		const first = await sent(s, "project.open");
		await s.fail(
			first,
			-32005,
			"has_team: that folder already has a Catervas team",
		);
		expect(
			await screen.findByText(
				"new-repo already has a Catervas team. It will be replaced by yours, starting fresh; your code is not touched.",
			),
		).toBeTruthy();
		// Choose another: back to the folder browser, nothing sent.
		fireEvent.click(screen.getByRole("button", { name: en.replaceNo }));
		await answerQuery(s, "folders.list", {
			path: "",
			parent: null,
			entries: [{ name: "new-repo", git: true }],
		});
		expect(await screen.findByRole("group", { name: en.folders })).toBeTruthy();
		expect(s.calls("project.open")).toHaveLength(1);

		fireEvent.click(
			within(screen.getByRole("group", { name: en.folders })).getByRole(
				"button",
				{ name: /new-repo/ },
			),
		);
		fireEvent.click(screen.getByRole("button", { name: en.useFolder }));
		await waitFor(() => expect(s.calls("project.open")).toHaveLength(2));
		await s.fail(
			s.calls("project.open")[1] as never,
			-32005,
			"has_team: that folder already has a Catervas team",
		);
		fireEvent.click(await screen.findByRole("button", { name: en.replaceYes }));
		await waitFor(() => expect(s.calls("project.open")).toHaveLength(3));
		const third = s.calls("project.open")[2];
		expect(third?.params).toEqual({
			path: "new-repo",
			no_sandbox: true,
			replace: true,
		});
		expect(refusedBy("projectOpenRequest", third?.params ?? {})).toEqual([]);
	});

	it("starts_a_new_project", async () => {
		const { socket, sockets, fetch } = await renderApp(
			"/setup/project",
			{ "GET /session": 204 },
			<Status />,
		);
		const s = socket as FakeSocket;
		await answerStatus(s, false, 1, NO_PROJECT);
		fireEvent.click(await screen.findByRole("radio", { name: /^No, start/ }));
		fireEvent.change(screen.getByLabelText(en.newWhat), {
			target: {
				value: "An ordering site for my bakery, where customers pick up.",
			},
		});
		fireEvent.change(screen.getByLabelText(en.newName), {
			target: { value: "Corner Bakery!" },
		});
		fireEvent.click(screen.getByRole("button", { name: en.continue }));
		const create = await sent(s, "project.create");
		expect(create.params).toEqual({
			parent: "",
			name: "corner-bakery",
			description: "An ordering site for my bakery, where customers pick up.",
			no_sandbox: false,
		});
		await s.reply(create, { project_root: "/home/me/corner-bakery" });
		expect(await screen.findByText(en.opening)).toBeTruthy();
		expect(screen.getByTestId("status").textContent).toBe("reopening");

		// The setup daemon stops: the page asks for its session again at once, not in 5 s.
		act(() => s.close());
		expect(screen.queryByText(en.lostTitle)).toBeNull();
		await waitFor(() => expect(sockets).toHaveLength(2));
		expect(fetch).toHaveBeenCalledTimes(2);
		const next = sockets[1] as FakeSocket;
		act(() => next.emit("open", {}));
		// "/" is Today's shell, which shows once Catervas answers.
		await answerStatus(next, true);
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
		expect(
			await screen.findByText(en.pausedNothingNew, { exact: false }),
		).toBeTruthy();
	});

	it("goes_back_to_an_earlier_step_from_the_list", async () => {
		const { socket } = await renderApp("/setup/scan");
		await answerStatus(socket as FakeSocket, false, 1, {
			setup_pending: true,
		});
		const steps = screen.getByRole("navigation", { name: en.setupSteps });
		fireEvent.click(
			within(steps).getByRole("button", { name: en.wizardAccount }),
		);
		expect(
			await screen.findByRole("heading", { name: en.accountTitle }),
		).toBeTruthy();
	});

	it("scan_goes_back_to_the_project", async () => {
		const { socket } = await renderApp("/setup/scan");
		await answerStatus(socket as FakeSocket, false, 1, {
			setup_pending: true,
		});
		fireEvent.click(await screen.findByRole("button", { name: en.back }));
		expect(
			await screen.findByRole("heading", { name: en.projectTitle }),
		).toBeTruthy();
	});

	it("keeps_the_project_already_taken_on", async () => {
		const first = await renderApp("/setup/project");
		await answerStatus(first.socket as FakeSocket, false, 1, {
			project_root: "/home/u/shop",
			setup_pending: true,
		});
		fireEvent.click(await screen.findByRole("button", { name: "Keep shop" }));
		expect(
			await screen.findByRole("heading", { name: en.scanTitle }),
		).toBeTruthy();
		cleanup();

		const second = await renderApp("/setup/project");
		await answerStatus(second.socket as FakeSocket, false, 1, {
			project_root: null,
			credential: null,
		});
		await screen.findByRole("heading", { name: en.projectTitle });
		expect(screen.queryByRole("button", { name: /^Keep / })).toBeNull();
		cleanup();

		// A project whose setup is finished is not offered either.
		const third = await renderApp("/setup/project");
		await answerStatus(third.socket as FakeSocket, false, 1, {
			project_root: "/home/u/shop",
			setup_pending: false,
		});
		await screen.findByRole("heading", { name: en.projectTitle });
		expect(screen.queryByRole("button", { name: /^Keep / })).toBeNull();
		cleanup();

		// Nor is it while the team is moving away from another project.
		const fourth = await renderApp("/setup/project");
		await answerStatus(fourth.socket as FakeSocket, false, 1, {
			project_root: "/home/u/shop",
			setup_pending: true,
			leaving: "/h/old-repo",
		});
		await screen.findByText("Moving your team from old-repo");
		expect(screen.queryByRole("button", { name: /^Keep / })).toBeNull();
	});

	it("sends_the_user_to_setup_without_a_project", async () => {
		const first = await renderApp("/");
		await answerStatus(first.socket as FakeSocket, false, 1, NO_PROJECT);
		expect(
			await screen.findByRole("heading", { name: en.computerTitle }),
		).toBeTruthy();
		cleanup();

		const second = await renderApp("/");
		await answerStatus(second.socket as FakeSocket, false, 1, {
			...NO_PROJECT,
			take_on_error: "the team's driver could not start",
		});
		expect(
			await screen.findByRole("heading", { name: en.projectTitle }),
		).toBeTruthy();
		// The project screen asks for the status too.
		await answerStatus(second.socket as FakeSocket, false, 1, {
			...NO_PROJECT,
			take_on_error: "the team's driver could not start",
		});
		expect((await screen.findByRole("alert")).textContent).toContain(
			"the team's driver could not start",
		);
	});

	it("offers_the_catervafication_at_the_end_of_setup", async () => {
		const { container } = await theOffer();
		expect(
			await screen.findByRole("heading", {
				level: 1,
				name: "Your team is ready",
			}),
		).toBeTruthy();
		expect(
			screen.getByText(
				"Mira, Sol, Ada, Theo and Kai have joined Corner Bakery orders. They know what Catervas found in your project; next, they can learn it properly.",
			),
		).toBeTruthy();
		const offer = screen.getByRole("region", {
			name: "Start with a Catervafication",
		});
		expect(within(offer).getByText("Recommended")).toBeTruthy();
		expect(
			within(offer).getByText(
				"Your team reads your project, writes down what it learns, looks for bugs and starts your marketing. You can read all of it in Files.",
			),
		).toBeTruthy();
		const rows = within(offer).getAllByRole("listitem");
		expect(rows.map((row) => row.textContent)).toEqual([
			"Mira PM writes what your product is and what comes next, as your product plan.",
			"Ada ARCH writes how your project is put together, with diagrams.",
			"Theo DEV writes how your project is built and worked on, and looks for bugs in the code.",
			"Kai MKT writes your marketing plan and starts on it; anything that costs money waits for you.",
		]);
		expect(within(offer).queryByText("Sol")).toBeNull();
		expect(
			within(offer).getByText(
				"Your team adds its notes and files each bug it finds as a request; nothing in your code changes yet.",
			),
		).toBeTruthy();
		expect(within(offer).getByText("Later keeps this on Today.")).toBeTruthy();
		// Every setup step is behind the owner.
		const steps = within(
			screen.getByRole("navigation", { name: en.setupSteps }),
		).getAllByRole("listitem");
		expect(steps).toHaveLength(8);
		for (const step of steps) expect(step.textContent).toContain(", done");
		await expectNoAxeViolations(container);
	});

	it("the_offer_says_the_team_hunts_and_markets", async () => {
		const team = {
			...TEAM,
			agents: [
				...TEAM.agents,
				agent("iris", "Iris", "ui_ux_designer", "designer"),
			],
		};
		await theOffer(
			{
				offered: true,
				roles: [
					"product_manager",
					"software_developer",
					"ui_ux_designer",
					"marketing_specialist",
				],
			},
			team,
		);
		const offer = await screen.findByRole("region", {
			name: "Start with a Catervafication",
		});
		expect(
			within(offer).getByText(
				"Your team reads your project, writes down what it learns, looks for bugs and starts your marketing. You can read all of it in Files.",
			),
		).toBeTruthy();
		const rows = within(offer)
			.getAllByRole("listitem")
			.map((row) => row.textContent);
		expect(rows).toContain(
			"Iris UX writes down every screen your project has, and looks for what is broken on them.",
		);
	});

	it("starts_or_leaves_it_for_later", async () => {
		const first = await theOffer();
		fireEvent.click(await screen.findByRole("button", { name: "Start" }));
		const command = await waitFor(() => {
			const frame = first.s.calls("command")[0];
			if (!frame) throw new Error("no command was sent");
			return frame;
		});
		expect(command.params).toEqual({
			command: { command: "catervafication_start", body: {} },
		});
		expect(refusedBy("emptyBody", bodyOf(command))).toEqual([]);
		await first.s.reply(command, { said: "started", events: [9] });
		await answerStatus(first.s, false);
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
		cleanup();

		// Later sends nothing.
		const second = await theOffer();
		fireEvent.click(await screen.findByRole("button", { name: "Later" }));
		await answerStatus(second.s, false);
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
		expect(second.s.calls("command")).toHaveLength(0);
		cleanup();

		// A refusal is said where the buttons are, and the offer stays.
		const third = await theOffer();
		fireEvent.click(await screen.findByRole("button", { name: "Start" }));
		const refused = await waitFor(() => {
			const frame = third.s.calls("command")[0];
			if (!frame) throw new Error("no command was sent");
			return frame;
		});
		await third.s.reply(refused, {
			error: {
				kind: "refused",
				detail:
					"catervafication_no_product_manager: the team has no active Product Manager",
			},
		});
		const alert = await screen.findByRole("alert");
		expect(alert.textContent).toBe(
			"Your Product Manager is paused. Resume them on the Team page, then start.",
		);
		// It is under the buttons.
		expect(
			alert.compareDocumentPosition(
				screen.getByRole("button", { name: "Later" }),
			) & Node.DOCUMENT_POSITION_PRECEDING,
		).toBeTruthy();
		expect(
			screen.getByRole("heading", { name: "Your team is ready" }),
		).toBeTruthy();
	});

	it("goes_home_when_nothing_is_offered", async () => {
		const { s } = await theOffer({ offered: false, roles: [] });
		await answerStatus(s, false);
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
		expect(
			screen.queryByRole("heading", { name: "Your team is ready" }),
		).toBeNull();
		expect(s.calls("command")).toHaveLength(0);
	});

	it("goes_home_when_the_offer_cannot_be_read", async () => {
		const rendered = await renderApp("/setup/ready");
		const s = rendered.socket as FakeSocket;
		const asked = await waitFor(() => {
			const frame = s
				.calls("query")
				.find((f) => f.params.name === "catervafication.get");
			if (!frame) throw new Error("no catervafication.get was asked");
			return frame;
		});
		await s.fail(asked, -32000, "the offer cannot be read");
		await answerStatus(s, false);
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
		expect(s.calls("command")).toHaveLength(0);
	});

	it("start_the_team_goes_to_the_offer", async () => {
		const { socket } = await renderApp("/setup/finish");
		const s = socket as FakeSocket;
		await answerQuery(s, "team.propose", {
			team: { ...TEAM, policy: { integration: "auto_merge" } },
			criteria: { criteria: [] },
		});
		await answerStatus(s, false, 1, { setup_pending: true });
		fireEvent.click(await screen.findByRole("button", { name: en.startTeam }));
		const start = await sent(s, "team.start");
		await s.reply(start, {});
		// The offer's own queries are asked: setup does not end at Today.
		await answerQuery(s, "catervafication.get", OFFER);
		await answerQuery(s, "team.get", { team: TEAM });
		expect(
			await screen.findByRole("heading", { name: "Your team is ready" }),
		).toBeTruthy();
	});

	it("joins_names_as_the_lead_does", () => {
		expect(joinedNames([])).toBe("");
		expect(joinedNames(["A"])).toBe("A");
		expect(joinedNames(["A", "B"])).toBe("A and B");
		expect(joinedNames(["A", "B", "C"])).toBe("A, B and C");
	});
});
