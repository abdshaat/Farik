// The finished task that the acceptance gate's and the help page's tests share.
import { waitFor } from "@testing-library/react";
import type { FakeSocket } from "./fake-socket.ts";
import { CONTRACT, TEAM } from "./plan.ts";
import { answerQuery, answerStatus, renderApp } from "./render-app.tsx";

const event = (
	seq: number,
	kind: string,
	body: object,
	at: string,
	agent_id?: string,
) => ({
	seq,
	recorded_at: at,
	team_id: "t",
	project_id: "p",
	task_id: "FRK-1",
	...(agent_id ? { agent_id } : {}),
	kind,
	body,
});
const cost = (seq: number, cost_usd: number) =>
	event(
		seq,
		"cost.recorded",
		{
			purpose: "implement",
			model_id: "m",
			usage: {},
			cost_usd,
		},
		"2026-09-25T08:00:00Z",
		"theo",
	);
const note = (
	seq: number,
	kind: string,
	text: string,
	by: string,
	at = "2026-09-25T09:00:00Z",
) => event(seq, "note.written", { kind, text, written_by: by }, at, by);

export const COMPLETION =
	"Customers can now buy a gift card for $25, $50 or $100 and pay for it.";
export const REVIEW =
	"It does what the plan asked. The receipt shows the card's code.";
/** A task that waits for the human's acceptance, as `task.history` answers it. */
export const HISTORY = [
	event(
		1,
		"task.created",
		{ summary: "Gift cards", created_by: "human" },
		"2026-09-23T09:00:00Z",
	),
	event(
		2,
		"human.accepted",
		{ subject: "contract", accepted_by: "human" },
		"2026-09-24T10:30:00Z",
	),
	cost(3, 1.2),
	note(4, "completion", "An earlier try.", "theo", "2026-09-25T07:00:00Z"),
	note(5, "progress", "Made the receipt show the code.", "theo"),
	note(6, "progress", "Ran the purchase test three times.", "theo"),
	note(
		7,
		"completion",
		`${COMPLETION}\n\nI did not change how prices are worked out.`,
		"theo",
	),
	note(8, "review", `${REVIEW}\n\nNothing else to add.`, "ada"),
	cost(9, 0.62),
	event(
		10,
		"escalation.raised",
		{
			reason: "iterations",
			detail: "The purchase test keeps timing out before the page is ready.",
		},
		"2026-09-25T07:52:00Z",
		"theo",
	),
];
export const DIFF = `diff --git a/src/gift.ts b/src/gift.ts
index 1..2 100644
--- a/src/gift.ts
+++ b/src/gift.ts
@@ -1 +1 @@
-old
+new`;
export const TASK = {
	...CONTRACT,
	kind: "task",
	status: "verifying",
	created_at: "2026-09-23T09:00:00Z",
};

/** The acceptance row `waiting.list` answers for FRK-1 while its result waits on the human. */
export const ACCEPTING = [
	{
		task_id: "FRK-1",
		kind: "acceptance",
		agent_id: "theo",
		title: "Gift cards",
		line: "Theo finished it and Ada reviewed it.",
	},
];

/** `path` for FRK-1, with each of `names` answered: the team, `contract`, its history, checks, diff, tries, `waiting` and choices, or what `overrides` gives. */
export async function openedGate(
	path: string,
	names: string[],
	contract: object = TASK,
	waiting: object[] = ACCEPTING,
	overrides: Record<string, unknown> = {},
) {
	const { container, socket } = await renderApp(path);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	const answers: Record<string, unknown> = {
		"team.get": { team: TEAM },
		"contract.get": { contract },
		"task.history": { events: HISTORY },
		"task.checks": {
			checks: [
				{
					criterion_id: "C1",
					text: "A test purchase of each amount goes through.",
					passed: true,
					evidence: "3 passed",
				},
			],
		},
		"task.diff": {
			diff: DIFF,
			files: ["src/gift.ts", "src/receipt.ts", "src/email.ts"],
			added: 142,
			removed: 18,
		},
		"task.tries": { try: 1, of: 4 },
		"waiting.list": { waiting },
		"escalation.choices": {
			choices: [
				{
					label: "Give 2 more tries",
					body: {
						command: "escalation_resolve",
						body: { task_id: "FRK-1", to: "in_progress", extra_tries: 2 },
					},
				},
				{
					label: "Ask Mira to change the plan",
					body: {
						command: "escalation_resolve",
						body: { task_id: "FRK-1", to: "refining" },
					},
				},
			],
		},
	};
	Object.assign(answers, overrides);
	for (const name of names) await answerQuery(s, name, answers[name]);
	return { container, s };
}

/** The command the page sent, once it has sent `count`. */
export const sentCommand = (s: FakeSocket, count = 1) =>
	waitFor(() => {
		const c = s.calls("command")[count - 1];
		if (!c) throw new Error(`fewer than ${count} commands were sent`);
		return c;
	});
