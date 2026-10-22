import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, renderApp } from "../test/render-app.tsx";

describe("this computer", () => {
	afterEach(() => {
		cleanup();
		vi.unstubAllGlobals();
	});

	it("sets_the_computer_s_limits", async () => {
		const { hub } = await renderApp("/computer");
		const h = hub as FakeSocket;
		await answerQuery(h, "computer.get", {
			agents_at_once: null,
			daily_cap_usd: null,
			spent_today_usd: 7.5,
		});
		const agents = (await screen.findByLabelText(
			en.computerAgentsAtOnce,
		)) as HTMLSelectElement;
		expect(agents.value).toBe("");
		expect(screen.getByText("Today, all teams: $7.50")).toBeTruthy();
		const cap = screen.getByLabelText(
			en.computerDailyLimit,
		) as HTMLInputElement;

		fireEvent.change(agents, { target: { value: "2" } });
		fireEvent.change(cap, { target: { value: "25" } });
		fireEvent.click(screen.getByRole("button", { name: en.computerSave }));
		const first = await waitFor(() => {
			const frame = h.calls("computer.set")[0];
			if (!frame) throw new Error("no computer.set was sent");
			return frame;
		});
		expect(first.params).toEqual({ agents_at_once: 2, daily_cap_usd: 25 });
		await h.reply(first, {});
		expect(await screen.findByText(en.computerSaved)).toBeTruthy();

		// "No limit" clears each.
		fireEvent.change(agents, { target: { value: "" } });
		fireEvent.change(cap, { target: { value: "" } });
		fireEvent.click(screen.getByRole("button", { name: en.computerSave }));
		const second = await waitFor(() => {
			const frame = h.calls("computer.set")[1];
			if (!frame) throw new Error("no second computer.set was sent");
			return frame;
		});
		expect(second.params).toEqual({
			agents_at_once: null,
			daily_cap_usd: null,
		});

		// A refusal is said in plain words, never as the daemon wrote it.
		await h.fail(
			second,
			-32005,
			"agents_at_once_out_of_range: agents at once is from 1 to 16",
		);
		const alert = await screen.findByRole("alert");
		expect(alert.textContent).toBe(en.refuseAgentsAtOnce);
	});
});
