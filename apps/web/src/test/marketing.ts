// The Marketing Specialist's team and the plan the marketing plan tests share, as the daemon answers.

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	status: "active",
});
export const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		agent("kai", "Kai", "marketing_specialist", "marketing-specialist"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};

/** Markup an agent can write into any of a plan's words: shown as typed, never made into an element. */
const MARKUP = "<b>x</b>";

export const TITLE = `Autumn at Corner Bakery ${MARKUP}`;
export const SUMMARY = `Six weeks to sell more Thanksgiving pies and bring new people in on Saturday mornings. The ads cost at most $450 in all, and the posts cost nothing. ${MARKUP}`;
/** What an agent can write in a plan's text: markup and a markdown heading, all to be shown as typed. */
export const TEXT =
	'# MP-3 Autumn at Corner Bakery\n## What I found\n- Last year you sold 74 Thanksgiving pies.\n<img src="x" onerror="alert(1)">\n<script>alert(1)</script>\n**not bold**';
export const NAME = `Pie pre-orders ${MARKUP}`;
export const GOAL = `Thanksgiving pie pre-orders from <i>people</i> searching ${MARKUP}`;
export const TOPIC = `Our autumn menu ${MARKUP}`;
/** What a campaign says it advertises: the agent's words, to be shown as typed. */
export const ADVERTISES = `Thanksgiving pies to pre-order: pumpkin, pecan and apple ${MARKUP}`;

/** MP-3 as `marketing_plan.get` answers it while it waits on the owner. */
export const PLAN = {
	plan: "MP-3",
	title: TITLE,
	summary: SUMMARY,
	text: TEXT,
	state: "proposed",
	starts_on: "2026-10-12",
	ends_on: "2026-11-22",
	currency: "USD",
	budget: { total: "450.00", google_ads: "450.00" },
	campaigns: [
		{
			key: "pies",
			channel: "google_ads",
			name: NAME,
			goal: GOAL,
			advertises: ADVERTISES,
			price: "fixed",
			budget: "300.00",
			starts_on: "2026-10-26",
			ends_on: "2026-11-22",
		},
		{
			key: "near-me",
			channel: "google_ads",
			name: "Bakery near me",
			goal: "New customers searching for a bakery within 2 miles",
			advertises: "The bakery itself: bread and pastries fresh from 7 am",
			price: "fixed",
			budget: "150.00",
			starts_on: "2026-10-12",
			ends_on: "2026-11-22",
		},
	],
	posts: [
		{
			key: "p1",
			channel: "instagram",
			on: "2026-10-12",
			topic: TOPIC,
		},
		{
			key: "p2",
			channel: "x",
			on: "2026-10-14",
			topic: "Pumpkin loaf is back",
		},
		{
			key: "p3",
			channel: "instagram",
			on: "2026-10-17",
			topic: "Shaping the sourdough",
		},
		{
			key: "p4",
			channel: "instagram",
			on: "2026-10-19",
			topic: "Meet the bakers",
		},
		{ key: "p5", channel: "x", on: "2026-10-25", topic: "A rainy week ahead" },
		{
			key: "p6",
			channel: "instagram",
			on: "2026-10-26",
			topic: "Pie pre-orders open",
		},
	],
	written_posts: [],
	measures: [
		`120 pie pre-orders by 22 November (last year: 74) ${MARKUP}`,
		"A pre-order from the ads costs under $4.00",
	],
	google_ads_account: "482-193-7720",
	replaces: null,
	agent_id: "kai",
	task_id: "FRK-31",
	proposed_at: "2026-10-06T08:40:00Z",
	decided: null,
	ended: null,
};
