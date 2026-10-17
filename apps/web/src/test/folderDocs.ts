// The product plan's changes Today asks the owner to approve, as `folder_doc_proposals.list`
// answers (spec 5.7), with the words of the approved mockup: Mira's two documents after the
// sprint 4 review.

import type { FakeSocket } from "./fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "./render-app.tsx";

const same = (text: string) => ({ change: "same", text });
const added = (text: string) => ({ change: "added", text });
const removed = (text: string) => ({ change: "removed", text });

const GIFT_CARDS = "- Gift cards. Buy a card online, spend it in the shop.";

/** The card of proposals 7 and 8, as the daemon words it. */
export const CARD = {
	sprint_id: "S4",
	agent_id: "mira",
	summary:
		"Pie pre-orders are done, so they move to Done and gift cards come next. I added the 3-day cut-off we agreed at the review.",
	documents: [
		{
			proposal: 7,
			path: "docs/catervas/product/roadmap.md",
			lines: [
				same("## Done"),
				added("- Pie pre-orders. 41 pies ordered in the first week."),
				same("## Now"),
				removed(
					"- Pie pre-orders. Customers order Thanksgiving pies ahead and collect them on 25 or 26 November.",
				),
				added(GIFT_CARDS),
				same("## Next"),
				removed(GIFT_CARDS),
				added(
					"- Christmas orders. Stollen, yule logs and mince pies, collected from 19 to 24 December.",
				),
				same("- Opening hours on every page."),
			],
		},
		{
			proposal: 8,
			path: "docs/catervas/product/spec.md",
			lines: [
				same("## Ordering ahead"),
				same("Customers pick a day and a time to collect their order."),
				added(
					"Pre-orders close 3 days before collection, so the kitchen can plan.",
				),
			],
		},
	],
};

/** What `folder_doc_proposals.list` answers for `cards` under `integration`. */
export const docs = (
	cards: unknown[] = [CARD],
	integration = "auto_merge",
) => ({ integration, cards });

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	status: "active",
});

export const TEAM = {
	name: "Corner Bakery",
	agents: [agent("mira", "Mira", "product_manager", "product-manager")],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};

/** Today with the team and each list answered: what waits on the owner, and the plan's changes. */
export async function todayWithDocs(fields: {
	docs?: unknown;
	waiting?: unknown[];
}) {
	const { container, socket } = await renderApp("/");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "team.activity", { activity: [] });
	await answerQuery(s, "waiting.list", { waiting: fields.waiting ?? [] });
	await answerQuery(s, "moved.since", { moved: [] });
	await answerQuery(s, "folder_doc_proposals.list", fields.docs ?? docs());
	return { container, s };
}
