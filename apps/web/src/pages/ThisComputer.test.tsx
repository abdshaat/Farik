import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerCalls, IDLE, row } from "../test/finding.ts";
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
	it("pauses_finding_and_forgets_everything", async () => {
		const { hub } = await renderApp("/computer");
		const h = hub as FakeSocket;
		const table: Record<string, unknown> = {
			"finding.status": {
				...IDLE,
				phase: "done",
				index_exists: true,
				ended_at: new Date().toISOString(),
			},
			"finding.ignored": {
				ignored: [
					row({
						key: "/home/me/code/awesome-llm-tools",
						name: "awesome-llm-tools",
					}),
					row({ key: "/home/me/code/old-landing", name: "old-landing" }),
				],
			},
			"finding.retention": { cleanup_period_days: 30, default: true },
			"finding.pause": { paused: true },
			"finding.start": { started: true },
			"finding.choose": {},
			"finding.keep_longer": { cleanup_period_days: 365 },
			"finding.forget": { forgotten: true },
		};
		answerCalls(h, table);
		await answerQuery(h, "computer.get", {
			agents_at_once: null,
			daily_cap_usd: null,
			spent_today_usd: 0,
		});

		// The daily look is on, says when it last looked, and pauses on the switch.
		const pause = await screen.findByRole("switch", {
			name: "Look for new projects every day",
		});
		expect(pause.getAttribute("aria-checked")).toBe("true");
		expect(screen.getByText(/^Last looked today at \d\d:\d\d\.$/)).toBeTruthy();
		table["finding.status"] = {
			...(table["finding.status"] as object),
			paused: true,
		};
		fireEvent.click(pause);
		const paused = await waitFor(() => {
			const frame = h.calls("finding.pause")[0];
			if (!frame) throw new Error("no finding.pause was sent");
			return frame;
		});
		expect(paused.params).toEqual({ paused: true });
		await waitFor(() =>
			expect(pause.getAttribute("aria-checked")).toBe("false"),
		);
		// While paused nothing starts; turned on again, Scan now starts a scan.
		expect(
			(screen.getByRole("button", { name: "Scan now" }) as HTMLButtonElement)
				.disabled,
		).toBe(true);
		table["finding.status"] = {
			...(table["finding.status"] as object),
			paused: false,
		};
		fireEvent.click(pause);
		await waitFor(() =>
			expect(pause.getAttribute("aria-checked")).toBe("true"),
		);
		fireEvent.click(screen.getByRole("button", { name: "Scan now" }));
		await waitFor(() => expect(h.calls("finding.start")).toHaveLength(1));

		// An ignored name comes back as a team or a watch.
		const ignored = (await screen.findByText("awesome-llm-tools")).closest(
			"li",
		) as HTMLElement;
		fireEvent.click(
			within(ignored).getByRole("button", { name: "Bring back…" }),
		);
		fireEvent.click(within(ignored).getByRole("button", { name: "Watch" }));
		const back = await waitFor(() => {
			const frame = h.calls("finding.choose")[0];
			if (!frame) throw new Error("no finding.choose was sent");
			return frame;
		});
		expect(back.params).toEqual({
			key: "/home/me/code/awesome-llm-tools",
			choice: "watch",
		});

		// A longer history is offered while Claude Code keeps the default, and written on the press.
		expect(
			screen.getByText("Claude Code deletes its history after 30 days."),
		).toBeTruthy();
		table["finding.retention"] = { cleanup_period_days: 365, default: false };
		fireEvent.click(screen.getByRole("button", { name: "Keep a year" }));
		const kept = await waitFor(() => {
			const frame = h.calls("finding.keep_longer")[0];
			if (!frame) throw new Error("no finding.keep_longer was sent");
			return frame;
		});
		expect(kept.params).toEqual({ days: 365 });
		await waitFor(() =>
			expect(screen.queryByRole("button", { name: "Keep a year" })).toBeNull(),
		);

		// Forgetting asks once, and says what stays.
		fireEvent.click(screen.getByRole("button", { name: "Forget everything…" }));
		const dialog = await screen.findByRole("dialog");
		expect(within(dialog).getByText("Forget everything?")).toBeTruthy();
		expect(
			within(dialog).getByText("Your teams, folders and Claude history stay."),
		).toBeTruthy();
		expect(h.calls("finding.forget")).toHaveLength(0);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Forget everything" }),
		);
		const forgot = await waitFor(() => {
			const frame = h.calls("finding.forget")[0];
			if (!frame) throw new Error("no finding.forget was sent");
			return frame;
		});
		expect(forgot.params).toEqual({ confirmed: true });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});
});
