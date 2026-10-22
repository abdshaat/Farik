import {
	cleanup,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { answerCalls, IDLE, NOTHING, row } from "../../test/finding.ts";
import {
	answerQuery,
	renderApp,
	teamList,
	where,
} from "../../test/render-app.tsx";

/** A repository on GitHub that no folder on this computer has, with Claude's work in it. */
const REPO = row({
	key: "github.com/octo/tidepool-web",
	name: "octo/tidepool-web",
	kind: "github",
	folders: [],
	last_at: "2026-10-20T08:00:00Z",
	suggested: "watch",
	claude_work: [
		{ kind: "branch", name: "claude/fix-login" },
		{ kind: "branch", name: "claude/add-search-7Qx" },
		{
			kind: "pull",
			name: "Fix the login",
			number: 12,
			draft: true,
		},
	],
	github: {
		full_name: "octo/tidepool-web",
		private: true,
		default_branch: "main",
		pushed_at: "2026-10-20T08:00:00Z",
		not_seen_lately: false,
	},
});

describe("github, for an owner who uses claude on the web", () => {
	afterEach(() => {
		cleanup();
		localStorage.clear();
	});

	it("connects_github_and_offers_a_copy", async () => {
		const { hub, go } = await renderApp("/find/github");
		const h = hub as FakeSocket;
		const kept = { connected: false } as Record<string, unknown>;
		answerCalls(h, {
			"finding.status": { ...IDLE, phase: "done", index_exists: true },
			"finding.list": { ...NOTHING, main: [REPO] },
			"github.status": () => kept,
			"github.connect": () => {
				kept.connected = true;
				kept.login = "octo";
				return { login: "octo" };
			},
			"github.copy": { folder: "/home/me/work/tidepool-web" },
			"github.disconnect": () => {
				kept.connected = false;
				delete kept.login;
				return {};
			},
		});

		// The page between the export and the services: what it reads, a token, and Skip.
		expect(
			await screen.findByText("Connect GitHub", { selector: "h1" }),
		).toBeTruthy();
		expect(screen.getByText("2 of 3 · optional")).toBeTruthy();
		expect(screen.getByText("We change nothing on GitHub.")).toBeTruthy();
		const link = screen.getByRole("link", { name: "Make a token on GitHub" });
		expect(link.getAttribute("href")).toBe(
			"https://github.com/settings/personal-access-tokens/new",
		);
		fireEvent.click(screen.getByRole("button", { name: "Skip" }));
		await waitFor(() => expect(where()).toBe("/find/services"));

		// A pasted token is sent, and the login it belongs to shows.
		go("/find/github");
		const field = await screen.findByLabelText("Your fine-grained token");
		fireEvent.change(field, { target: { value: "ghp_invented_token" } });
		fireEvent.click(screen.getByRole("button", { name: "Connect GitHub" }));
		const connect = await waitFor(() => {
			const frame = h.calls("github.connect")[0];
			if (!frame) throw new Error("no github.connect was sent");
			return frame;
		});
		expect(connect.params).toEqual({ token: "ghp_invented_token" });
		expect(await screen.findByText("Connected as octo.")).toBeTruthy();
		// The token is not shown again.
		expect(screen.queryByDisplayValue("ghp_invented_token")).toBeNull();

		// A repository not on this computer offers Watch and Copy, and no Team.
		go("/find/found");
		await answerQuery(h, "teams.list", teamList("shop"));
		await screen.findByText("octo/tidepool-web", { selector: "strong" });
		const item = screen
			.getByText("octo/tidepool-web", { selector: "strong" })
			.closest("li") as HTMLElement;
		const mine = within(item);
		expect(mine.getByText("On GitHub, not on this computer")).toBeTruthy();
		expect(
			mine.getByText(
				"2 branches from Claude on the web · 1 pull request (draft)",
			),
		).toBeTruthy();
		expect(
			(mine.getByRole("radio", { name: "Watch" }) as HTMLInputElement).checked,
		).toBe(true);
		expect(mine.queryByRole("radio", { name: "Team" })).toBeNull();
		fireEvent.click(
			mine.getByRole("button", { name: "Copy to this computer" }),
		);

		// The owner picks the folder to put it in; the copy is asked for and said.
		await answerQuery(h, "folders.list", {
			path: "",
			parent: null,
			entries: [{ name: "work", git: false }],
		});
		fireEvent.click(await screen.findByRole("button", { name: /work/ }));
		fireEvent.click(screen.getByRole("button", { name: "Open folder" }));
		await answerQuery(h, "folders.list", {
			path: "work",
			parent: "",
			entries: [],
		});
		await waitFor(() =>
			expect(screen.getByText("~/work", { selector: "span" })).toBeTruthy(),
		);
		fireEvent.click(screen.getByRole("button", { name: "Use this folder" }));
		const copy = await waitFor(() => {
			const frame = h.calls("github.copy")[0];
			if (!frame) throw new Error("no github.copy was sent");
			return frame;
		});
		expect(copy.params).toEqual({
			full_name: "octo/tidepool-web",
			parent: "work",
		});
		expect(
			await screen.findByText("Copied to ~/work/tidepool-web."),
		).toBeTruthy();

		// This computer says whose GitHub it is, and Disconnect forgets the key.
		go("/computer");
		await answerQuery(h, "computer.get", {
			agents_at_once: null,
			daily_cap_usd: null,
			spent_today_usd: 0,
		});
		expect(await screen.findByText("GitHub: connected as octo")).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Disconnect" }));
		await waitFor(() => expect(h.calls("github.disconnect")).toHaveLength(1));
		expect(
			await screen.findByRole("button", { name: "Connect GitHub" }),
		).toBeTruthy();
	});
});
