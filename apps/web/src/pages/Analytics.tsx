import { Segmented } from "@catervas/ui";
import { useState } from "react";
import { t } from "../strings/t.ts";
import styles from "./Analytics.module.css";
import {
	Blockers,
	ClaudeAiLine,
	Unpriced,
	UsageBars,
	UsageNote,
	usageRows,
	WorkflowPanel,
} from "./AnalyticsParts.tsx";
import { type Period, useAnalytics } from "./analytics.ts";

/** Analytics on All teams: usage per team, the workflow, and the blockers (spec F17). */
export function Analytics() {
	const [period, setPeriod] = useState<Period>("week");
	const data = useAnalytics(period);
	return (
		<div className={styles.page}>
			<div className={styles.head}>
				<h1 className={styles.title}>{t("analyticsTitle")}</h1>
				<Segmented<Period>
					name="analytics-period"
					legend={t("analyticsPeriod")}
					options={[
						{ value: "week", label: t("analyticsWeek") },
						{ value: "month", label: t("analyticsMonth") },
					]}
					value={period}
					onChange={setPeriod}
				/>
			</div>
			{data && (
				<>
					<section className={styles.panel} aria-labelledby="usage-h">
						<div className={styles.titled}>
							<h2 id="usage-h">{t("analyticsUsage")}</h2>
							<UsageNote billing={data.billing} id="usage-info" />
						</div>
						<p className={styles.muted}>
							{t(
								data.billing === "api"
									? "analyticsApiNote"
									: "analyticsSpentNote",
							)}
						</p>
						<UsageBars
							caption={t("analyticsUsageTable")}
							rows={usageRows(data.teams, data.watchedUsd)}
						/>
						<Unpriced
							count={data.teams.reduce(
								(sum, team) => sum + team.unpricedReplies,
								0,
							)}
						/>
						<ClaudeAiLine count={data.claudeAiConversations} period={period} />
					</section>
					<section className={styles.panel} aria-labelledby="workflow-h">
						<h2 id="workflow-h">{t("analyticsWorkflow")}</h2>
						<WorkflowPanel workflow={data.workflow} period={period} />
					</section>
					<section className={styles.panel} aria-labelledby="blockers-h">
						<h2 id="blockers-h">{t("analyticsBlockers")}</h2>
						<Blockers blockers={data.blockers} />
					</section>
				</>
			)}
		</div>
	);
}
