// A marketing plan's ads and their budget, as the daemon answers (ADR 0042, step 08g): the rows
// Today lists when a budget was reached, when ads may be running that Catervas could not pause, or
// when it cannot read the spend, and the running plan whose page tells the same.

import { NAME, PLAN, TITLE } from "./marketing.ts";

/** What Catervas read from Google Ads at 10:15, and each campaign's share of it. */
const CAMPAIGNS = [
	{ key: "pies", name: NAME, budget: "300.00", spent: "168.65" },
	{ key: "near-me", name: "Bakery near me", budget: "150.00", spent: "150.00" },
];

/** Bakery near me reached its own budget, and Catervas paused it. */
const CAMPAIGN_CAP = {
	scope: "campaign",
	key: "near-me",
	name: "Bakery near me",
	spent: "150.00",
	budget: "150.00",
};

/** The whole plan reached its Google Ads budget, and Catervas paused its ads. */
const PLAN_CAP = { scope: "plan", spent: "450.00", budget: "450.00" };

const PLAN_FIELDS = {
	task_id: "FRK-31",
	agent_id: "kai",
	plan: "MP-3",
	plan_title: TITLE,
	ends_on: "2026-11-22",
	currency: "USD",
	google_ads: "450.00",
};

/** The Google Ads words the daemon keeps when Google refuses a pause. */
export const REFUSED =
	"Google answered “The service is currently unavailable.”";

/** The row `waiting.list` gives once Bakery near me reached its budget and Catervas paused it. */
export const BUDGET_ROW = {
	...PLAN_FIELDS,
	kind: "marketing_budget",
	title: `Ads budget reached: ${TITLE}`,
	line: "Its campaign Bakery near me reached its budget: 150.00 of 150.00 USD. Catervas paused it.",
	spent: "318.65",
	read_at: "2026-11-12T10:15:00Z",
	cap: CAMPAIGN_CAP,
	caps: [CAMPAIGN_CAP],
	campaigns: CAMPAIGNS,
};

/** The plan's own budget was reached, and Google refused the pause. */
export const PLAN_BUDGET_ROW = {
	...BUDGET_ROW,
	line: `Its ads reached their budget: 450.00 of 450.00 USD. Catervas could not pause them: ${REFUSED} Catervas tries again every 15 minutes; pause them in Google Ads.`,
	spent: "450.00",
	reason: REFUSED,
	cap: PLAN_CAP,
	caps: [PLAN_CAP],
	campaigns: CAMPAIGNS.map((one) => ({ ...one, spent: one.budget })),
};

/** An ended plan whose ads Google would not pause. */
export const RUNNING_ROW = {
	...PLAN_FIELDS,
	kind: "marketing_ads_running",
	title: `Ads still running: ${TITLE}`,
	line: `Catervas could not pause its ads: ${REFUSED} They keep running at Google until 2026-11-22 or their budget there. Pause them in Google Ads.`,
	reason: REFUSED,
};

/** A running plan whose spend Catervas cannot read, with the last spend it could. */
export const UNREAD_ROW = {
	...PLAN_FIELDS,
	kind: "marketing_spend_unread",
	title: `Can't read the ad spend: ${TITLE}`,
	line: "Catervas can't read its ad spend: Kai's sign-in to Google has ended; sign Kai in again on Kai's page. Any of its ads still running keep running at Google until 2026-11-22 or their budget there; pause them in Google Ads.",
	reason: "Kai's sign-in to Google has ended; sign Kai in again on Kai's page",
	spent: "318.65",
	read_at: "2026-11-12T10:15:00Z",
};

/** MP-3 as `marketing_plan.get` answers it while it runs, with a read at 10:15 and one pause. */
export const RUNNING_PLAN = {
	...PLAN,
	state: "active",
	starts_on: "2026-10-12",
	ends_on: "2026-11-22",
	decided: {
		decision: "approved",
		note: "Looks good.",
		at: "2026-10-06T19:05:00Z",
	},
	spend: {
		read_at: "2026-11-12T10:15:00Z",
		total: "318.65",
		by_key: { pies: "168.65", "near-me": "150.00" },
	},
	reached: [
		{
			scope: "campaign",
			key: "near-me",
			spent: "150.00",
			budget: "150.00",
			at: "2026-11-12T10:15:00Z",
		},
	],
	paused: [
		{
			key: "near-me",
			name: "Bakery near me",
			why: "budget_reached",
			at: "2026-11-12T10:15:00Z",
		},
	],
};
