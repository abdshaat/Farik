import { useEffect, useState } from "react";
import { type Connection, useConnection, useHub } from "../app/connection.tsx";

/** The period an answer covers. */
export type Period = "week" | "month";

/** A team's usage in the period, in US dollars. */
export type TeamUsage = {
	slug: string;
	name: string;
	ownerUsd: number;
	agentsUsd: number;
	unpricedReplies: number;
};

/** The workflow numbers of the period. */
export type Workflow = {
	sessions: number;
	activeHours: number;
	commits: { owner: number; team: number };
	pullRequests?: number;
	usedMost: { label: string; calls: number }[];
	perDay: { day: string; sessions: number }[];
};

/** One kind of blocker, with how many and some of them named. */
export type Blocker = {
	kind:
		| "waiting_on_you"
		| "schedules"
		| "deploys"
		| "blocked"
		| "forgotten"
		| "denials";
	count: number;
	examples: string[];
};

/** The `analytics.get` answer. */
export type AnalyticsAll = {
	period: Period;
	/** `api` on a subscription: the amounts are what the use would cost on Claude's API. */
	billing: "api" | "spent";
	teams: TeamUsage[];
	watchedUsd: number;
	claudeAiConversations: number;
	workflow: Workflow;
	blockers: Blocker[];
};

/** The `analytics.mine` answer. */
export type AnalyticsMine = {
	period: Period;
	billing: "api" | "spent";
	team: TeamUsage;
	workflow: Workflow;
	blockers: Blocker[];
	cycleDays: number | null;
	throughput: number;
};

/** What `method` answers for `period`, asked again when the period changes. */
function useAnswer<T>(
	client: Connection["client"],
	method: "analytics.get" | "analytics.mine",
	period: Period,
): T | undefined {
	const [answer, setAnswer] = useState<{ period: Period; data: T }>();
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.call(method, { period }).then(
			(data) => live && setAnswer({ period, data: data as T }),
			() => {},
		);
		return () => {
			live = false;
		};
	}, [client, method, period]);
	return answer?.period === period ? answer.data : undefined;
}

/** Every team's analytics, asked of the hub. */
export function useAnalytics(period: Period): AnalyticsAll | undefined {
	return useAnswer<AnalyticsAll>(useHub().client, "analytics.get", period);
}

/** The current team's analytics, asked of its daemon. */
export function useTeamAnalytics(period: Period): AnalyticsMine | undefined {
	return useAnswer<AnalyticsMine>(
		useConnection().client,
		"analytics.mine",
		period,
	);
}
