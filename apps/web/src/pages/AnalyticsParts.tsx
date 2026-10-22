import { BarChart, type BarRow, InfoTip } from "@catervas/ui";
import { Bullets } from "../components/Bullets.tsx";
import { t } from "../strings/t.ts";
import styles from "./Analytics.module.css";
import type { Blocker, Period, TeamUsage, Workflow } from "./analytics.ts";
import { dollars } from "./Gate.tsx";

/** An amount as the page writes it beside a bar: nothing is "$0", the rest has its cents. */
const money = (n: number) => (n === 0 ? "$0" : dollars(n));

/** The words naming a count of conversations. */
const conversations = (count: number) =>
	count === 1
		? t("analyticsConversationOne")
		: t("analyticsConversations", { count });

/** One bar of usage: the owner's amount and the agents', or the owner's alone for a watched folder. */
export type UsageRow = {
	key: string;
	label: string;
	ownerUsd: number;
	agentsUsd: number | null;
};

/** The usage rows of `teams`, then the folders no team holds when they were used. */
export function usageRows(teams: TeamUsage[], watchedUsd: number): UsageRow[] {
	const rows: UsageRow[] = teams.map((team) => ({
		key: team.slug,
		label: team.name,
		ownerUsd: team.ownerUsd,
		agentsUsd: team.agentsUsd,
	}));
	if (watchedUsd > 0)
		rows.push({
			key: "watched",
			label: t("analyticsWatched"),
			ownerUsd: watchedUsd,
			agentsUsd: null,
		});
	return rows;
}

/** Usage as stacked bars, "You" then "The team's agents", each bar's two amounts written beside it. */
export function UsageBars({
	caption,
	rows,
}: {
	caption: string;
	rows: UsageRow[];
}) {
	const bars: BarRow[] = rows.map((row) => ({
		key: row.key,
		label: row.label,
		values: [row.ownerUsd, row.agentsUsd ?? 0],
		text:
			row.agentsUsd === null
				? money(row.ownerUsd)
				: `${money(row.ownerUsd)} + ${money(row.agentsUsd)}`,
	}));
	return (
		<BarChart
			caption={caption}
			series={[
				{ key: "you", label: t("analyticsYou") },
				{ key: "agents", label: t("analyticsAgents") },
			]}
			rows={bars}
			direction="across"
			format={dollars}
			tableSummary={t("analyticsTable")}
			labelColumn={t("analyticsTeam")}
		/>
	);
}

/** What the amounts are: the API's price for a subscription, else what was spent. */
export function UsageNote({
	billing,
	id,
}: {
	billing: "api" | "spent";
	id: string;
}) {
	return (
		<>
			<InfoTip id={id}>
				<Bullets
					text={t(billing === "api" ? "analyticsApiTip" : "analyticsSpentTip")}
				/>
			</InfoTip>
		</>
	);
}

/** The line that says how many replies had no price to count. */
export function Unpriced({ count }: { count: number }) {
	if (count === 0) return null;
	return (
		<p className={styles.muted}>
			{count === 1
				? t("analyticsUnpricedOne")
				: t("analyticsUnpriced", { count })}
		</p>
	);
}

/** claude.ai conversations count as activity alone: the export has no usage figures. */
export function ClaudeAiLine({
	count,
	period,
}: {
	count: number;
	period: Period;
}) {
	return (
		<p className={styles.muted}>
			{t("analyticsClaudeAi", {
				what: conversations(count),
				when: t(period === "week" ? "analyticsThisWeek" : "analyticsThisMonth"),
			})}
		</p>
	);
}

/** A number with what it counts under it. */
export function Stat({ value, label }: { value: string; label: string }) {
	return (
		<div className={styles.stat}>
			<b>{value}</b> <span>{label}</span>
		</div>
	);
}

const WEEKDAYS = [
	"Sunday",
	"Monday",
	"Tuesday",
	"Wednesday",
	"Thursday",
	"Friday",
	"Saturday",
];

/** The sessions of each day as columns: weekdays for a week, the day of the month for a month. */
function dayRows(perDay: Workflow["perDay"], period: Period): BarRow[] {
	return perDay.map(({ day, sessions }) => {
		const [year = 0, month = 1, date = 1] = day.split("-").map(Number);
		const weekday =
			WEEKDAYS[new Date(Date.UTC(year, month - 1, date)).getUTCDay()] ?? day;
		return {
			key: day,
			label: period === "week" ? weekday : String(date),
			// A month has too many columns to name them all: every fifth day is.
			short:
				period === "week"
					? weekday.slice(0, 1)
					: date % 5 === 1
						? String(date)
						: "",
			values: [sessions],
			text: String(sessions),
		};
	});
}

/** The workflow: the four numbers, the sessions each day, and the tools used most. */
export function WorkflowPanel({
	workflow,
	period,
}: {
	workflow: Workflow;
	period: Period;
}) {
	return (
		<>
			<div className={styles.stats}>
				<Stat
					value={String(workflow.sessions)}
					label={t("analyticsSessions")}
				/>
				<Stat
					value={t("analyticsHours", { hours: workflow.activeHours })}
					label={t("analyticsActive")}
				/>
				<Stat
					value={`${workflow.commits.owner} · ${workflow.commits.team}`}
					label={t("analyticsCommits")}
				/>
				{workflow.pullRequests !== undefined && (
					<Stat
						value={String(workflow.pullRequests)}
						label={t("analyticsPulls")}
					/>
				)}
			</div>
			<div className={styles.pair}>
				<div className={styles.cell}>
					<p className={styles.muted}>{t("analyticsPerDay")}</p>
					<BarChart
						caption={t("analyticsPerDay")}
						series={[{ key: "sessions", label: t("analyticsSessionsSeries") }]}
						rows={dayRows(workflow.perDay, period)}
						direction="up"
						format={String}
						tableSummary={t("analyticsTable")}
						labelColumn={t("analyticsDay")}
					/>
				</div>
				<div className={styles.cell}>
					<p className={styles.muted}>{t("analyticsUsedMost")}</p>
					<ul className={styles.used}>
						{workflow.usedMost.map((tool) => (
							<li key={tool.label}>
								{tool.label} · {tool.calls}
							</li>
						))}
					</ul>
				</div>
			</div>
		</>
	);
}

const BLOCKER_WORDS: Record<Blocker["kind"], Parameters<typeof t>[0]> = {
	waiting_on_you: "analyticsBlockerWaiting",
	schedules: "analyticsBlockerSchedules",
	deploys: "analyticsBlockerDeploys",
	blocked: "analyticsBlockerBlocked",
	forgotten: "analyticsBlockerForgotten",
	denials: "analyticsBlockerDenials",
};

/** One row to a kind of blocker, with its count and its examples. */
export function Blockers({ blockers }: { blockers: Blocker[] }) {
	if (blockers.length === 0)
		return <p className={styles.muted}>{t("analyticsNoBlockers")}</p>;
	return (
		<ul className={styles.blockers}>
			{blockers.map((blocker) => (
				<li key={blocker.kind}>
					<strong>
						{t(BLOCKER_WORDS[blocker.kind])} · {blocker.count}
					</strong>
					<span>{blocker.examples.join(", ")}</span>
				</li>
			))}
		</ul>
	);
}
