import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	persona: `${name} persona`,
	status: "active",
	model: { id: "claude-opus-5", effort: "high" },
});
const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		agent("sol", "Sol", "scrum_master", "scrum-master"),
		agent("ada", "Ada", "architect", "architect"),
		agent("theo", "Theo", "software_developer", "developer"),
		agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
	],
	budgets: {},
	policy: {
		integration: "auto_merge",
		permissions: { run_commands: true, push: false },
	},
	rules: {},
};
const MODELS = {
	models: [
		{ id: "claude-opus-5", label: "Strongest model, thinks hard" },
		{ id: "claude-sonnet-5", label: "Everyday model" },
	],
};

/** Opens `path` with the team and the models answered. */
async function opened(path: string) {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "models.list", MODELS);
	return { container, s };
}

/** The latest request of `method` the page sent. */
async function sent(socket: FakeSocket, method: string) {
	return waitFor(() => {
		const frame = socket.calls(method).at(-1);
		if (!frame) throw new Error(`no ${method} was sent`);
		return frame;
	});
}

/** The latest `team.validate` the page asked, answered with `effects`. */
async function validated(s: FakeSocket, effects: string[]) {
	const frame = await waitFor(() => {
		const f = s
			.calls("query")
			.filter((q) => q.params.name === "team.validate")
			.at(-1);
		if (!f) throw new Error("no team.validate was asked");
		return f;
	});
	act(() => s.reply(frame, { errors: [], effects }));
	return (frame.params.params as { team: typeof TEAM }).team;
}

const saved = async (s: FakeSocket) =>
	((await sent(s, "team.save")).params as { team: typeof TEAM }).team;
const one = (team: typeof TEAM, id: string) =>
	team.agents.find((a) => a.id === id) as Record<string, unknown>;

describe("team page", () => {
	afterEach(() => localStorage.clear());

	it("lists_the_team_with_the_first_day_line", async () => {
		const { container, s } = await opened("/team");
		const list = await screen.findByRole("list", { name: en.teamMembers });
		expect(within(list).getAllByRole("listitem")).toHaveLength(5);
		expect(within(list).getByText("Ada persona")).toBeTruthy();
		expect(
			within(list).getAllByText("Strongest model, thinks hard"),
		).toHaveLength(5);
		expect(
			within(list)
				.getByRole("link", { name: "Edit Theo" })
				.getAttribute("href"),
		).toBe("/team/theo");
		expect(screen.getByText(en.firstDay, { exact: false })).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("button", { name: "Pause Theo" }));
		expect((await sent(s, "command")).params).toEqual({
			command: {
				command: "agent_update",
				body: { agent_id: "theo", status: "paused" },
			},
		});
	});

	it("edits_an_agent_and_shows_the_effect_first", async () => {
		const { container, s } = await opened("/team/theo");
		expect(
			await screen.findByRole("heading", { name: "Theo, your Developer" }),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(screen.getByRole("radio", { name: /^Quick/ }));
		const checked = await validated(s, ["Theo now thinks with low effort."]);
		expect(one(checked, "theo").model).toEqual({
			id: "claude-opus-5",
			effort: "low",
		});
		expect(
			await screen.findByText("Theo now thinks with low effort."),
		).toBeTruthy();
		expect(s.calls("team.save")).toHaveLength(0);

		fireEvent.click(screen.getByRole("button", { name: en.agentSave }));
		expect(one(await saved(s), "theo").model).toEqual({
			id: "claude-opus-5",
			effort: "low",
		});
	});

	it("toggles_tiers_in_advanced", async () => {
		const { container, s } = await opened("/team/ada");
		await screen.findByRole("heading", { name: "Ada, your Architect" });
		expect(screen.queryByRole("switch", { name: en.tierNetwork })).toBeNull();

		fireEvent.click(screen.getByRole("switch", { name: en.advancedSwitch }));
		const tiers = screen.getByRole("list", { name: "What Ada may do" });
		expect(within(tiers).getAllByRole("switch")).toHaveLength(7);
		const network = within(tiers).getByRole("switch", {
			name: en.tierNetwork,
		});
		expect(network.getAttribute("aria-checked")).toBe("true");
		expect(
			within(tiers)
				.getByRole("switch", { name: en.tierGitRemote })
				.getAttribute("aria-checked"),
		).toBe("false");
		await expectNoAxeViolations(container);

		fireEvent.click(network);
		expect(network.getAttribute("aria-checked")).toBe("false");
		await validated(s, ["Ada may no longer use the internet."]);
		fireEvent.click(screen.getByRole("button", { name: en.agentSave }));
		expect(one(await saved(s), "ada").revokes).toEqual(["network"]);
	});

	it("replaces_an_agent", async () => {
		const { s } = await opened("/team/theo");
		fireEvent.click(
			await screen.findByRole("button", { name: "Replace Theo" }),
		);
		const retire = await sent(s, "command");
		expect(retire.params).toEqual({
			command: {
				command: "agent_update",
				body: { agent_id: "theo", status: "retired" },
			},
		});
		expect(s.calls("team.save")).toHaveLength(0);
		act(() => s.reply(retire, { said: "Theo retired", events: [7] }));

		const team = await saved(s);
		expect(one(team, "theo").status).toBe("retired");
		expect(one(team, "noor")).toMatchObject({
			display_name: "Noor",
			role: "software_developer",
			status: "active",
		});
		act(() => s.reply(s.calls("team.save")[0] as never, {}));
		// The Team page asks for the team again.
		const again = await waitFor(() => {
			const f = s.calls("query").filter((q) => q.params.name === "team.get");
			if (f.length < 2) throw new Error("the team was not asked again");
			return f.at(-1) as never;
		});
		act(() => s.reply(again, { team: TEAM }));
		expect(
			await screen.findByRole("heading", { name: en.teamMembers }),
		).toBeTruthy();
	});

	it("shows_and_disconnects_the_account", async () => {
		const { container, socket } = await renderApp("/settings");
		const s = socket as FakeSocket;
		await answerStatus(s, false);
		await answerQuery(s, "account.status", {
			provider: "anthropic",
			kind: "api_key",
			source: "keychain",
		});
		const row = await screen.findByRole("region", { name: en.accountRow });
		expect(within(row).getByText(/an API key/)).toBeTruthy();
		expect(within(row).getByText(en.accountKeychain)).toBeTruthy();
		await expectNoAxeViolations(container);

		fireEvent.click(
			within(row).getByRole("button", { name: en.accountDisconnect }),
		);
		const gone = await sent(s, "account.disconnect");
		expect(gone.params).toEqual({});
		act(() => s.reply(gone, { removed_from: ["keychain"], paused: true }));
		expect(await within(row).findByText(en.accountGone)).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
