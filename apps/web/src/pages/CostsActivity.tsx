import { t } from "../strings/t.ts";
import styles from "./Analytics.module.css";
import { Stat, Unpriced, UsageBars, UsageNote } from "./AnalyticsParts.tsx";
import { useTeamAnalytics } from "./analytics.ts";

/** A mean number of days to a tenth, as the page says it. */
function days(mean: number): string {
	const tenths = Math.round(mean * 10) / 10;
	return tenths === 1
		? t("analyticsCycleDay")
		: t("analyticsCycleDays", { days: tenths });
}

/** The Activity tab of a team's Costs: the week's usage split, its task cycle time and what it finished. */
export function CostsActivity() {
	const data = useTeamAnalytics("week");
	if (!data) return null;
	const waiting =
		data.blockers.find((blocker) => blocker.kind === "waiting_on_you")?.count ??
		0;
	return (
		<div className={styles.panel}>
			<div className={styles.titled}>
				<p className={styles.muted}>
					{t(
						data.billing === "api" ? "analyticsApiNote" : "analyticsSpentNote",
					)}
				</p>
				<UsageNote billing={data.billing} id="activity-info" />
			</div>
			<UsageBars
				caption={t("analyticsUsageTable")}
				rows={[
					{
						key: "week",
						label: t("analyticsWeek"),
						ownerUsd: data.team.ownerUsd,
						agentsUsd: data.team.agentsUsd,
					},
				]}
			/>
			<Unpriced count={data.team.unpricedReplies} />
			<div className={styles.stats}>
				<Stat
					value={
						data.cycleDays === null ? t("metricNotYet") : days(data.cycleDays)
					}
					label={t("analyticsCycle")}
				/>
				<Stat
					value={String(data.throughput)}
					label={t(
						data.throughput === 1 ? "analyticsDoneOne" : "analyticsDone",
					)}
				/>
				<Stat
					value={`${data.workflow.commits.owner} · ${data.workflow.commits.team}`}
					label={t("analyticsCommits")}
				/>
				<Stat value={String(waiting)} label={t("analyticsWaiting")} />
			</div>
		</div>
	);
}
