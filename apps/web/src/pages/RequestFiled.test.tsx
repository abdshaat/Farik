import { expectNoAxeViolations } from "@farik/ui/test";
import {
	act,
	fireEvent,
	screen,
	waitFor,
	within,
} from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";

const agent = (id: string, name: string, role: string) => ({
	id,
	display_name: name,
	role,
	avatar: role,
	status: "active",
});
const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager"),
		agent("sol", "Sol", "scrum_master"),
		agent("theo", "Theo", "software_developer"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};
const INTENT =
	"Let customers buy gift cards they can send to a friend by email, and use them when they order.";
const REASON =
	"This touches paying, email and the order page, so it is more than one piece of work.";
const event = (seq: number, kind: string, body: object, at: string) => ({
	seq,
	recorded_at: at,
	team_id: "t",
	project_id: "p",
	task_id: "FRK-7",
	kind,
	body,
});
const CREATED = event(
	3,
	"task.created",
	{ summary: "Gift cards", created_by: "human" },
	"2026-09-26T09:14:00Z",
);
const TRIAGED = event(
	4,
	"request.triaged",
	{ size: "large", reason: REASON, triaged_by: "sol" },
	"2026-09-26T09:15:00Z",
);

/** The request page for FRK-7, with the team, the contract and its history answered. */
async function opened(status: string, events: object[]) {
	const { container, socket } = await renderApp("/requests/FRK-7");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "contract.get", {
		contract: { id: "FRK-7", title: "Gift cards", intent: INTENT, status },
	});
	await answerQuery(s, "task.history", { events });
	return { container, s };
}

const asked = (s: FakeSocket, name: string) =>
	s.calls("query").filter((q) => q.params.name === name);

describe("request page", () => {
	it("shows_the_request_and_its_size", async () => {
		const { container, s } = await opened("draft", [CREATED]);
		expect(
			await screen.findByRole("heading", { name: en.requestTitle }),
		).toBeTruthy();
		expect(screen.getByText("FRK-7, sent at 09:14 UTC.")).toBeTruthy();
		expect(screen.getByText(INTENT)).toBeTruthy();
		// Before triage, the Scrum Master is still sizing it.
		expect(screen.getByText("Sol is sizing your request…")).toBeTruthy();
		expect(asked(s, "task.history")[0]?.params.params).toEqual({
			task_id: "FRK-7",
		});

		// The triage arrives; the page asks again and shows the size.
		act(() => s.event(5));
		await waitFor(() => expect(asked(s, "task.history")).toHaveLength(2));
		await answerQuery(s, "task.history", { events: [CREATED, TRIAGED] });
		expect(
			await screen.findByRole("heading", {
				name: "Sol sized it as a big request",
			}),
		).toBeTruthy();
		expect(screen.getByText("Sol, your Scrum Master, decided")).toBeTruthy();
		expect(screen.getByText(REASON)).toBeTruthy();
		expect(screen.getByText("This is what Sol chose.")).toBeTruthy();
		expect(
			screen.getByRole("heading", { name: en.sizeLargeCard }),
		).toBeTruthy();
		expect(
			screen.getByRole("heading", { name: en.sizeSmallCard }),
		).toBeTruthy();
		const next = screen.getByRole("list", { name: en.nextTitle });
		expect(within(next).getAllByRole("listitem")).toHaveLength(4);
		expect(
			within(next).getByText("Sol splits it into tasks and the team starts."),
		).toBeTruthy();
		const about = screen.getByRole("region", { name: en.aboutRequest });
		for (const text of [
			"Saturday 26 September",
			"Sol, Scrum Master",
			"Mira, Product Manager",
			"FRK-7",
			"Planning",
		])
			expect(within(about).getByText(text)).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("resizes_it", async () => {
		const { container, s } = await opened("draft", [CREATED, TRIAGED]);
		fireEvent.click(
			await screen.findByRole("button", { name: en.resizeToSmall }),
		);
		const sent = await waitFor(() => {
			const c = s.calls("command")[0];
			if (!c) throw new Error("no command was sent");
			return c;
		});
		expect(sent.params).toEqual({
			command: {
				command: "request_triage",
				body: { task_id: "FRK-7", size: "small", reason: "Changed by you" },
			},
		});
		act(() =>
			s.reply(sent, {
				error: {
					kind: "refused",
					detail: "triage_closed: the request is already being planned",
				},
			}),
		);
		expect((await screen.findByRole("alert")).textContent).toBe(
			"The request is already being planned",
		);
		await expectNoAxeViolations(container);

		// Once refining starts, the size is the team's: no button.
		act(() => s.event(6));
		await waitFor(() => expect(asked(s, "contract.get")).toHaveLength(2));
		await answerQuery(s, "contract.get", {
			contract: {
				id: "FRK-7",
				title: "Gift cards",
				intent: INTENT,
				status: "refining",
			},
		});
		await waitFor(() =>
			expect(screen.queryByRole("button", { name: en.resizeToSmall })).toBe(
				null,
			),
		);
	});
});
