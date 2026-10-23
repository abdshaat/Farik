const day = (n: number) => `2026-10-${String(19 + n)}`;

/** The week's sessions per day, Monday to Sunday: 4, 6, 3, 5, 2, 1, 2. */
const PER_DAY = [4, 6, 3, 5, 2, 1, 2].map((sessions, at) => ({
	day: day(at),
	sessions,
}));

const WORKFLOW = {
	sessions: 23,
	active_hours: 17,
	commits: { owner: 31, team: 12 },
	pull_requests: 6,
	used_most: [
		{ label: "Editing files", calls: 412 },
		{ label: "Running commands", calls: 280 },
		{ label: "Zoho Books", calls: 96 },
	],
	per_day: PER_DAY,
};

const team = (
	slug: string,
	name: string,
	ownerUsd: number,
	agentsUsd: number,
) => ({
	slug,
	name,
	owner_usd: ownerUsd,
	agents_usd: agentsUsd,
	unpriced_replies: 0,
});

/** The `analytics.get` answer of the mockup's first frame, for a subscription. */
export const ANALYTICS = {
	period: "week",
	billing: "api",
	teams: [
		team("tidepool", "Tidepool", 41.2, 18.6),
		team("price-watch", "Price Watch", 6.1, 2.4),
		team("bookkeeping", "Bookkeeping", 3.8, 0),
	],
	watched_usd: 1.3,
	claude_ai_conversations: 14,
	workflow: WORKFLOW,
	blockers: [
		{ kind: "waiting_on_you", count: 1, examples: ["Tidepool, 2 days"] },
		{
			kind: "schedules",
			count: 2,
			examples: ["Hourly price check", "Weekly database backup"],
		},
		{ kind: "deploys", count: 1, examples: ["tidepool-web"] },
		{ kind: "blocked", count: 1, examples: ["Tidepool: “Add Apple sign-in”"] },
	],
};

/** The `analytics.mine` answer of the mockup's second frame. */
export const MINE = {
	period: "week",
	billing: "api",
	team: team("shop", "Tidepool", 41.2, 18.6),
	workflow: { ...WORKFLOW, commits: { owner: 14, team: 12 } },
	blockers: [
		{ kind: "waiting_on_you", count: 1, examples: ["Tidepool, 2 days"] },
	],
	cycle_days: 1.6,
	throughput: 9,
};
