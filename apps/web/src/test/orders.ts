// The Procurement Specialist's orders and renewals as the daemon answers (spec 6.10, ADR 0039).
// Times are built in the browser's own time zone, so that the days the page says do not depend on
// where the tests run. "Today" is `NOW`, Monday 26 October 2026.

import { vi } from "vitest";
import type { FakeSocket } from "./fake-socket.ts";
import { answerQuery, answerStatus, renderApp } from "./render-app.tsx";
import { at, EFFECTIVE, MARKUP, NOW, TEAM } from "./sites.ts";

export { at, EFFECTIVE, MARKUP, NOW, TEAM };

/** A time of the given month (0 is January) of 2026 in the browser's time zone, as the daemon words it. */
export const on = (month: number, day: number, hour = 9) =>
	new Date(2026, month, day, hour).toISOString();

const LINES = [
	{
		item: "Printed pie box, 10 × 10 × 2.5 in, white, your logo in one colour",
		quantity: 500,
		unit: "boxes",
		unit_price: "2.40",
		line_total: "1200.00",
	},
	{
		item: "Printing plate for your logo",
		quantity: 1,
		unit: "",
		unit_price: "85.00",
		line_total: "85.00",
	},
	{
		item: "Delivery to Corner Bakery",
		quantity: 1,
		unit: "",
		unit_price: "165.00",
		line_total: "165.00",
	},
];

/** The row `waiting.list` gives while Ivo's order PO-12 waits for the owner, drafted this morning. */
export const ORDER_ROW = {
	task_id: "FRK-31",
	kind: "purchase_order",
	agent_id: "ivo",
	title: "Find a supplier for 500 pie boxes",
	line: "Ivo set up an order from Pie Box Pros: 1450.00 USD",
	order: 12,
	seller: "Pie Box Pros",
	seller_contact: "Dana Ruiz, sales, +1 555 0142",
	lines: LINES,
	currency: "USD",
	period: "once",
	total: "1450.00",
	delivery:
		"Ships 12 working days after you approve the proof of your logo, so it arrives by 28 October.",
	terms:
		"Paid by card on their site when you order. Prices held until 31 October.",
	url: "https://pieboxpros.com/printed-pie-boxes",
	host: "pieboxpros.com",
	evaluation: "evaluations/pie-boxes.md",
	why: "Lowest cost of the three quotes for 500 printed boxes once delivery is counted, and the only one that arrives before your pie week starts on 1 November.",
	at: at(26, 8),
	expires_at: on(10, 25, 8),
};

/** An order whose every text holds what must never become markup, or hide what it says. */
export const WRITTEN_ROW = {
	...ORDER_ROW,
	seller: `Pie Box ${MARKUP} Pros\u202e`,
	seller_contact: `Dana ${MARKUP}\u202e`,
	lines: [
		{
			item: `Box ${MARKUP}\u202e`,
			quantity: 5,
			unit: "bo‮xes",
			unit_price: "1.00",
			line_total: "5.00",
		},
	],
	total: "5.00",
	delivery: `Soon ${MARKUP}\u202e`,
	terms: `Net 30 ${MARKUP}\u202e`,
	url: "https://pieboxpros.com/a‮b",
	why: `Cheapest ${MARKUP} and ‮this`,
};

/** A name written in another alphabet, as Catervas keeps it. */
export const SCRIPT_ORDER_ROW = {
	...ORDER_ROW,
	order: 14,
	seller: "Uline",
	host: "xn--ulne-m9d.com",
	url: "https://ulіne.com/pie-boxes",
};

/** A monthly order that names no page, with seven lines. */
export const LONG_ORDER_ROW = {
	...ORDER_ROW,
	order: 15,
	seller: "Flour Club",
	period: "month",
	seller_contact: "",
	delivery: "",
	terms: "",
	url: "",
	host: undefined,
	lines: Array.from({ length: 7 }, (_, index) => ({
		item: `Flour ${index + 1}`,
		quantity: 2,
		unit: "bags",
		unit_price: "10.00",
		line_total: "20.00",
	})),
	total: "140.00",
};

/** What the evaluation of PO-12 says. */
export const COMPARISON = `# Three quotes\n\n<b>Pie Box Pros</b> is cheapest.\n${MARKUP}\u202e`;

const base = {
	state: "approved",
	seller: "Pie Box Pros",
	total: "1450.00",
	currency: "USD",
	period: "once",
	task_id: "FRK-31",
	agent_id: "ivo",
	drafted_at: at(26, 8),
	overdue: false,
};

/** `purchase_orders.list` as it answers for Ivo's page. */
export const ORDERS = {
	orders: [
		// The most recent five that ended, and one more.
		{
			...base,
			order: 4,
			state: "received",
			seller: "Old Mill",
			drafted_at: on(8, 1),
			decided_at: on(8, 2),
			placed_on: "2026-09-03",
			received_on: "2026-09-10",
			paid: "40.00",
			paid_currency: "USD",
			ended_at: on(8, 10),
		},
		{
			...base,
			order: 6,
			state: "expired",
			seller: "Bake Supply Co",
			drafted_at: on(8, 1),
			ended_at: on(8, 20),
		},
		{
			...base,
			order: 7,
			state: "expired",
			seller: `Bake ${MARKUP} Co`,
			drafted_at: on(8, 1),
			decided_at: on(8, 2),
			ended_at: on(8, 21),
		},
		{
			...base,
			order: 8,
			state: "rejected",
			seller: "Box & Bag Co",
			decided_at: on(8, 25),
			ended_at: on(8, 25),
		},
		{
			...base,
			order: 9,
			state: "received",
			seller: "Pie Tin Supply",
			total: "96.00",
			placed_on: "2026-09-20",
			received_on: "2026-09-30",
			paid: "88.50",
			paid_currency: "EUR",
			decided_at: on(8, 19),
			ended_at: on(8, 30),
		},
		{
			...base,
			order: 5,
			state: "closed",
			seller: "Cog & Co",
			placed_on: "2026-09-03",
			decided_at: on(8, 4),
			ended_at: on(9, 12),
		},
		// The orders that are not over.
		{
			...base,
			order: 10,
			state: "placed",
			seller: "Kitchen Parts Direct",
			total: "318.00",
			placed_on: "2026-09-18",
			decided_at: on(8, 15),
			overdue: true,
			status: {
				status: "shipped",
				note: "They emailed me: it left their warehouse on 26 September.",
				by: "owner",
				at: on(8, 28),
				expected_on: "2026-10-01",
			},
		},
		{
			...base,
			order: 11,
			state: "placed",
			seller: "Northfield Mill",
			total: "212.40",
			placed_on: "2026-10-02",
			paid: "190.00",
			paid_currency: "EUR",
			decided_at: on(9, 1),
			status: {
				status: "delayed",
				note: `The mill’s order page says flour is short this week. ${MARKUP}`,
				by: "agent",
				at: at(25, 10),
				expected_on: "2026-10-30",
			},
		},
		{
			...base,
			order: 13,
			state: "placed",
			seller: "Tin Town",
			total: "75.00",
			placed_on: "2026-09-20",
			decided_at: on(8, 19),
			overdue: true,
		},
		{
			...base,
			order: 12,
			decided_at: at(26, 8, 30),
			expires_at: on(10, 25, 8),
		},
	],
};

/** One renewal as `renewals.list` gives it. */
export const renewal = (
	n: number,
	vendor: string,
	renews: string,
	by: string,
) => ({
	renewal: n,
	vendor,
	renews_on: renews,
	decide_by: by,
	flagged_at: at(25, 6),
});

/** `renewals.list` with two renewals coming up and two rows Catervas cannot read. */
export const RENEWALS = {
	open: [
		renewal(7, "Vercel", "2026-11-15", "2026-10-16"),
		renewal(9, "Mailchimp", "2026-10-27", "2026-10-26"),
	],
	unreadable: 2,
};

/** Today at `NOW` with Ivo on the team, the lists answered, and the daemon's `renewals.list`. */
export async function todayWithOrders(fields: {
	waiting?: unknown[];
	renewals?: unknown;
}) {
	vi.useFakeTimers({ toFake: ["Date"] });
	vi.setSystemTime(NOW);
	const { container, socket } = await renderApp("/");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "team.activity", { activity: [] });
	await answerQuery(s, "waiting.list", { waiting: fields.waiting ?? [] });
	await answerQuery(s, "moved.since", { moved: [] });
	await answerQuery(s, "sprint.current", null);
	await answerQuery(s, "social_posts.list", { posts: [] });
	await answerQuery(
		s,
		"renewals.list",
		fields.renewals ?? { open: [], unreadable: 0 },
	);
	return { container, s };
}
