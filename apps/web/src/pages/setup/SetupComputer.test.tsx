import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
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
});
