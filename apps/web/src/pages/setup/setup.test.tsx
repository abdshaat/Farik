import { expectNoAxeViolations } from "@farik/ui/test";
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

const READY = { state: "ready", version: "2.1.300" };
const MISSING = { state: "missing" };
const NO_PROJECT = { project_root: null, credential: null };

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
		expect(screen.getByText(en.noDockerWarning)).toBeTruthy();
		const onward = screen.getByRole("button", { name: en.continue });
		expect((onward as HTMLButtonElement).disabled).toBe(true);
		await expectNoAxeViolations(first.container);

		fireEvent.click(
			screen.getByRole("button", { name: en.continueWithoutDocker }),
		);
		expect(
			await screen.findByRole("heading", { name: en.accountTitle }),
		).toBeTruthy();
		expect(sessionStorage.getItem("farik.noSandbox")).toBe("true");
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
		act(() => again.reply(build, { image: "farik/sandbox:1" }));
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
		act(() =>
			s.fail(refused, -32005, "a subscription key starts with sk-ant-oat"),
		);
		const why = await screen.findByText(
			"a subscription key starts with sk-ant-oat",
		);
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
		act(() => s.reply(call, { stored_in: "file", taking_on: false }));
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
		act(() => again.reply(kept, { stored_in: "keychain", taking_on: false }));
		expect(await screen.findByText(en.storedKeychain)).toBeTruthy();
		expect(screen.queryByText(en.storedFile)).toBeNull();
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
		act(() => s.reply(call, { stored_in: "file", taking_on: true }));
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
		sessionStorage.setItem("farik.noSandbox", "true");
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
		act(() => s.reply(create, { project_root: "/home/me/corner-bakery" }));
		expect(await screen.findByText(en.opening)).toBeTruthy();
		expect(screen.getByTestId("status").textContent).toBe("reopening");

		// The setup daemon stops: the page asks for its session again at once, not in 5 s.
		act(() => s.close());
		expect(screen.queryByText(en.lostTitle)).toBeNull();
		await waitFor(() => expect(sockets).toHaveLength(2));
		expect(fetch).toHaveBeenCalledTimes(2);
		const next = sockets[1] as FakeSocket;
		act(() => next.emit("open", {}));
		// "/" is Today's shell, which shows once Farik answers.
		await answerStatus(next, true);
		expect(await screen.findByRole("heading", { name: en.today })).toBeTruthy();
		expect(
			await screen.findByText(en.pausedNothingNew, { exact: false }),
		).toBeTruthy();
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
});
