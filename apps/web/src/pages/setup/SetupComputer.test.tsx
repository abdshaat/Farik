import { expectNoAxeViolations } from "@catervas/ui/test";
import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { en } from "../../strings/en.ts";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { answerQuery, renderApp } from "../../test/render-app.tsx";

const READY = { state: "ready", version: "2.1.300" };

describe("SetupComputer", () => {
	afterEach(() => sessionStorage.clear());

	it("shows_the_designer_browser_row", async () => {
		const { container, socket } = await renderApp("/setup/computer");
		const s = socket as FakeSocket;
		await answerQuery(s, "computer.check", {
			claude: READY,
			git: READY,
			docker: { state: "ready" },
			sandbox_image: { state: "ready" },
			designer_browser: { state: "missing" },
		});
		const row = (await screen.findByText(en.computerBrowser)).closest("li");
		if (!row) throw new Error("no browser row");
		expect(within(row).getByText(en.notFound)).toBeTruthy();
		expect(within(row).getByText(en.browserMissing)).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(within(row).getByRole("button", { name: en.fetchIt }));
		const pull = await waitFor(() => {
			const frame = s.calls("browser.pull").at(-1);
			if (!frame) throw new Error("no browser.pull was sent");
			return frame;
		});
		expect(pull.params).toEqual({});
		await s.reply(pull, { image: "mcr.microsoft.com/playwright/mcp" });
		// The browser is looked for again once it is fetched.
		await waitFor(() =>
			expect(
				s.calls("query").filter((f) => f.params.name === "computer.check"),
			).toHaveLength(2),
		);
	});

	it("shows_the_safe_box_being_prepared", async () => {
		const { container, socket } = await renderApp("/setup/computer");
		const s = socket as FakeSocket;
		const checks = () =>
			s.calls("query").filter((f) => f.params.name === "computer.check");
		const base = { claude: READY, git: READY, docker: { state: "ready" } };
		await answerQuery(s, "computer.check", {
			...base,
			sandbox_image: { state: "missing" },
		});
		fireEvent.click(await screen.findByRole("button", { name: en.prepare }));
		const build = await waitFor(() => {
			const frame = s.calls("sandbox.build").at(-1);
			if (!frame) throw new Error("no sandbox.build was sent");
			return frame;
		});
		await s.reply(build, { image: "catervas-sandbox" });

		// The next check says it is preparing, at step 2 of 4.
		await waitFor(() => expect(checks()).toHaveLength(2));
		await s.reply(checks()[1] as never, {
			...base,
			sandbox_image: { state: "preparing", done: 2, of: 4 },
		});
		const bar = await screen.findByRole("progressbar", {
			name: en.imagePreparingLabel,
		});
		expect(bar.getAttribute("value")).toBe("2");
		expect(bar.getAttribute("max")).toBe("4");
		expect(screen.getByText(/Step 2 of 4/)).toBeTruthy();
		expect(screen.queryByRole("button", { name: en.prepare })).toBeNull();
		expect(
			(screen.getByRole("button", { name: en.continue }) as HTMLButtonElement)
				.disabled,
		).toBe(true);
		await expectNoAxeViolations(container);

		// It asks again by itself, and a build with no step shows a bar without a measure.
		await waitFor(() => expect(checks()).toHaveLength(3), { timeout: 3000 });
		await s.reply(checks()[2] as never, {
			...base,
			sandbox_image: { state: "preparing" },
		});
		await waitFor(() =>
			expect(
				screen
					.getByRole("progressbar", { name: en.imagePreparingLabel })
					.hasAttribute("value"),
			).toBe(false),
		);
		expect(screen.getByText(en.imagePreparing)).toBeTruthy();

		// Ready comes by itself: Continue is enabled with no press of "Check again".
		await waitFor(() => expect(checks()).toHaveLength(4), { timeout: 3000 });
		await s.reply(checks()[3] as never, {
			...base,
			sandbox_image: { state: "ready" },
		});
		await waitFor(() =>
			expect(
				(screen.getByRole("button", { name: en.continue }) as HTMLButtonElement)
					.disabled,
			).toBe(false),
		);
		expect(screen.queryByRole("progressbar")).toBeNull();
		await expectNoAxeViolations(container);
		cleanup();

		// A build that failed says so, keeps Docker's words behind an info button, and offers it again.
		const failed = await renderApp("/setup/computer");
		await answerQuery(failed.socket as FakeSocket, "computer.check", {
			...base,
			sandbox_image: { state: "missing", failed: "boom" },
		});
		expect((await screen.findByRole("alert")).textContent).toContain(
			en.setupBuildFailed,
		);
		expect(screen.getByText("boom")).toBeTruthy();
		expect(screen.getByRole("button", { name: en.prepare })).toBeTruthy();
		await expectNoAxeViolations(failed.container);
	}, 15000);
});
