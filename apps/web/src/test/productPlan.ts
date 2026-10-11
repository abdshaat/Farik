// A new project's interview and product plan, as `chat.messages` and `product_plan.state` answer
// them (spec 4.3), with the words of the approved mockups: Mira's summary of the talk about
// Little Oak Cakes.

import type { FakeSocket } from "./fake-socket.ts";
import { TEAM } from "./folderDocs.ts";
import { answerQuery, answerStatus, renderApp } from "./render-app.tsx";

/** What Mira heard, as the chat's `heard` has it on the wire. */
export const HEARD = {
	customers:
		"People nearby ordering a cake for a birthday, a wedding or an office party.",
	problem:
		"Orders arrive by Instagram and phone, some get lost, and you can’t see the week ahead.",
	must: "Let customers choose a size, a flavour and a collection day and pay a deposit; show you the week’s orders in one list.",
	not_now: "Delivery and a phone app.",
	works_when:
		"No order is lost, and half your orders come through the website by March.",
};

/** The state while Mira interviews: a new project, no plan task filed yet. */
export const INTERVIEWING = {
	new_project: { name: "little-oak-cakes" },
	approved: false,
	task: null,
	documents: [],
};

/** The state once the plan is on the integration branch. */
export const APPROVED = { ...INTERVIEWING, approved: true };

const SPEC = "docs/catervas/product/spec.md";
const ROADMAP = "docs/catervas/product/roadmap.md";

/** Mira's product description and roadmap, as the mockup draws them. */
export const DOCUMENTS = [
	{
		path: SPEC,
		text: [
			"# Little Oak Cakes",
			"",
			"A website where people order a celebration cake from Little Oak Cakes and collect it from the shop.",
			"",
			"## Who it is for",
			"",
			"People nearby ordering a cake for a birthday, a wedding or an office party.",
			"",
			"## What it must do",
			"",
			"- Customers choose a size, a flavour and a collection day, and pay a 30% deposit.",
			"- You see every order for the week in one list, by collection day.",
			"",
			"## Not now",
			"",
			"- Delivery.",
			"- A phone app. The website works on phones.",
			"",
		].join("\n"),
	},
	{
		path: ROADMAP,
		text: [
			"## Now",
			"",
			"- **Order a cake online.** Size, flavour, collection day and the deposit.",
			"",
			"## Next",
			"",
			"- **The week’s orders.** One list for you, by collection day.",
			"",
			"## Later",
			"",
			"- **Gift vouchers.**",
			"",
		].join("\n"),
	},
];

/** This morning at 09:30 in the viewer's time, whatever day the test runs on. */
const thisMorning = () => {
	const at = new Date();
	at.setHours(9, 30, 0, 0);
	return at.toISOString();
};

/** The plan written and waiting for the owner: its task verifying, both documents on its branch. */
export const IN_REVIEW = {
	...INTERVIEWING,
	task: { task_id: "CTV-2", status: "verifying", created_at: thisMorning() },
	documents: DOCUMENTS,
};

/** The acceptance row `waiting.list` holds for that task. */
export const PRODUCT_PLAN_ROW = {
	task_id: "CTV-2",
	kind: "acceptance",
	agent_id: "mira",
	title: "Draft the product plan",
	line: "Accept the result",
};

/** Today with the team and each list answered, and the product plan's state as `state`. */
export async function todayWithPlan(fields: {
	state?: unknown;
	waiting?: unknown[];
	integration?: string;
}) {
	const { container, socket } = await renderApp("/");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", {
		team: {
			...TEAM,
			policy: { integration: fields.integration ?? "auto_merge" },
		},
	});
	await answerQuery(s, "team.activity", { activity: [] });
	await answerQuery(s, "waiting.list", {
		waiting: fields.waiting ?? [PRODUCT_PLAN_ROW],
	});
	await answerQuery(s, "moved.since", { moved: [] });
	await answerQuery(s, "sprint.current", null);
	await answerQuery(s, "product_plan.state", fields.state ?? IN_REVIEW);
	return { container, s };
}
