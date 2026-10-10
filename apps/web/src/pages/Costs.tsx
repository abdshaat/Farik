import { Avatar, type AvatarKey, Button } from "@catervas/ui";
import { useState } from "react";
import { useQuery } from "../app/store.ts";
import { useWide } from "../shell/Shell.tsx";
import { t } from "../strings/t.ts";
import { type AllowanceRow, countSaid, useAllowances } from "./allowances.tsx";
import board from "./Board.module.css";
import own from "./Costs.module.css";
import { DailyLimit } from "./dialogs/DailyLimit.tsx";
import { dollars } from "./Gate.tsx";
import styles from "./pages.module.css";
import { roleName, type Team } from "./setup/TeamSetup.tsx";
import type { RoleKit } from "./Team.tsx";

type Summary = {
	todayUsd: number;
	dailyLimitUsd?: number | null;
	sprint: {
		sprintId: string;
		status: "open" | "ended";
		spentUsd: number;
		budgetUsd?: number | null;
	} | null;
	agents: { agentId: string; todayUsd: number; sprintUsd: number }[];
	conversationsTodayUsd: number;
};
type Activity = { agentId: string; line: string };
type Words = { words: string; usd: number };
type Metrics = {
	acceptedTasks: number;
	firstPassAcceptanceRate: number | null;
	interventionsPerAcceptedTask: number | null;
	costPerAcceptedTask: { totalUsd: number; byPurpose: Words[] } | null;
	mechanicallyVerifiedCriteriaShare: number | null;
	activeWeeks: number;
	messages: Record<
		"reaction" | "ambient" | "reply" | "ceremony" | "system" | "human",
		number
	>;
};

/** Today's and this sprint's spending per agent, and the harness metrics (spec F17). */
export function Costs() {
	const { data: team } = useQuery<{ team: Team; kits?: RoleKit[] }>(
		"team.get",
		{},
	);
	// What agents made on other services; the page draws without it.
	const allowances = useAllowances();
	const wide = useWide();
	const { data: summary } = useQuery<Summary>("costs.summary", {});
	const { data: activity } = useQuery<{ activity: Activity[] }>(
		"team.activity",
		{},
	);
	const [oneSprint, setOneSprint] = useState(false);
	const sprint = summary?.sprint;
	const { data: metrics } = useQuery<Metrics>(
		"metrics",
		oneSprint && sprint ? { sprintId: sprint.sprintId } : {},
	);
	const [limiting, setLimiting] = useState(false);
	if (!team || !summary || !activity || !metrics) return null;
	const n = sprint?.sprintId.slice(1) ?? "";
	const ended = sprint?.status === "ended";
	const today = dollars(summary.todayUsd);

	const rate = (
		key: Parameters<typeof t>[0],
		value: string | null,
		note?: string,
	) => (
		<div key={key}>
			<dt>{t(key)}</dt>
			<dd className={value === null ? undefined : own.value}>
				{value ?? t("metricNotYet")}
			</dd>
			{value !== null && note && <dd className={styles.muted}>{note}</dd>}
		</div>
	);
	const m = metrics;
	const cost = m.costPerAcceptedTask;
	const whoMade = (row: AllowanceRow) =>
		team.team.agents.find((a) => a.id === row.agent)?.displayName ?? row.agent;
	const serviceOf = (row: AllowanceRow) =>
		(team.kits ?? [])
			.flatMap((kit) => kit.connectors)
			.find((service) => service.name === row.server)?.title ?? row.server;
	const period = t(
		allowances?.period.kind === "day" ? "allowCostsDay" : "allowCostsSprint",
	);

	return (
		<div className={styles.page}>
			<div className={board.head}>
				<h1 className={board.title}>{t("costs")}</h1>
				<p>
					{summary.dailyLimitUsd
						? t("costsTodayOf")
								.replace("{spent}", today)
								.replace("{limit}", dollars(summary.dailyLimitUsd))
						: t("costsToday").replace("{spent}", today)}
				</p>
				{sprint && (
					<>
						<p>
							{(sprint.budgetUsd
								? t(ended ? "costsSprintEnded" : "costsSprint").replace(
										"{budget}",
										dollars(sprint.budgetUsd),
									)
								: t(ended ? "costsSprintEndedNoLimit" : "costsSprintNoLimit")
							)
								.replace("{n}", n)
								.replace("{spent}", dollars(sprint.spentUsd))}
						</p>
						<p className={styles.muted}>{t("costsSprintWhy")}</p>
					</>
				)}
				<Button onClick={() => setLimiting(true)}>{t("costsSetLimit")}</Button>
			</div>
			<section className={styles.section} aria-labelledby="by-agent">
				<h2 id="by-agent">{t("costsByAgent")}</h2>
				<table className={own.table} aria-labelledby="by-agent">
					<thead>
						<tr>
							<th scope="col">{t("costsAgent")}</th>
							<th scope="col">{t("costsTodayColumn")}</th>
							<th scope="col">
								{ended
									? t("costsSprintEndedColumn").replace("{n}", n)
									: t("costsSprintColumn")}
							</th>
							<th scope="col">{t("costsNow")}</th>
						</tr>
					</thead>
					<tbody>
						{summary.agents.map((row) => {
							const agent = team.team.agents.find((a) => a.id === row.agentId);
							const name = agent?.displayName ?? row.agentId;
							return (
								<tr key={row.agentId}>
									<th scope="row">
										<span className={own.agent}>
											{agent?.avatar && (
												<Avatar
													avatarKey={agent.avatar as AvatarKey}
													name={name}
													size={32}
												/>
											)}
											<span>{name}</span>
										</span>
										{agent && (
											<span className={board.small}>
												{roleName(agent.role)}
											</span>
										)}
									</th>
									<td>{dollars(row.todayUsd)}</td>
									<td>{dollars(row.sprintUsd)}</td>
									<td>
										{activity.activity.find((a) => a.agentId === row.agentId)
											?.line ?? ""}
									</td>
								</tr>
							);
						})}
					</tbody>
				</table>
				<p className={styles.muted}>{t("costsResting")}</p>
				<p>
					{t("costsConversations")}{" "}
					<strong>{dollars(summary.conversationsTodayUsd)}</strong>.{" "}
					{t("costsConversationsWhy")}
				</p>
			</section>
			{allowances && allowances.rows.length > 0 && (
				<section className={styles.section} aria-labelledby="made-elsewhere">
					<h2 id="made-elsewhere">{t("allowBoardHeading")}</h2>
					{wide ? (
						<table className={own.table} aria-labelledby="made-elsewhere">
							<thead>
								<tr>
									<th scope="col">{t("allowCostsAgent")}</th>
									<th scope="col">{t("allowCostsService")}</th>
									<th scope="col">{t("allowCostsMade")}</th>
									<th scope="col">{t("allowCostsWhen")}</th>
								</tr>
							</thead>
							<tbody>
								{allowances.rows.map((row) => (
									<tr key={`${row.agent}-${row.server}-${row.tool}`}>
										<th scope="row">{whoMade(row)}</th>
										<td>{serviceOf(row)}</td>
										<td>{countSaid(row)}</td>
										<td>{period}</td>
									</tr>
								))}
							</tbody>
						</table>
					) : (
						<ul className={own.made} aria-labelledby="made-elsewhere">
							{allowances.rows.map((row) => (
								<li key={`${row.agent}-${row.server}-${row.tool}`}>
									<p>
										{t("allowCostsWho", {
											name: whoMade(row),
											service: serviceOf(row),
										})}
									</p>
									<p>{countSaid(row)}</p>
									<p>{period}</p>
								</li>
							))}
						</ul>
					)}
					<p className={styles.muted}>{t("allowCostsBill")}</p>
				</section>
			)}
			<section className={styles.section} aria-labelledby="how-well">
				<h2 id="how-well">{t("costsHowWell")}</h2>
				<p className={styles.muted}>
					{oneSprint && sprint
						? t("costsSprintOnly").replace("{n}", n)
						: t("costsWholeProject")}
				</p>
				{sprint && (
					<span>
						<button
							type="button"
							className={board.chip}
							aria-pressed={oneSprint}
							onClick={() => setOneSprint(!oneSprint)}
						>
							{t("costsShowSprint").replace("{n}", n)}
						</button>
					</span>
				)}
				<dl className={own.metrics}>
					{rate(
						"metricFirstTry",
						m.firstPassAcceptanceRate === null
							? null
							: t("metricFirstTryValue")
									.replace(
										"{first}",
										String(
											Math.round(m.firstPassAcceptanceRate * m.acceptedTasks),
										),
									)
									.replace("{accepted}", String(m.acceptedTasks)),
						// Only when some were sent back: "the rest" of "9 of 9" is none.
						m.firstPassAcceptanceRate !== null && m.firstPassAcceptanceRate < 1
							? t("metricFirstTryNote")
							: undefined,
					)}
					{rate(
						"metricNeeded",
						m.interventionsPerAcceptedTask === null
							? null
							: String(Math.round(m.interventionsPerAcceptedTask * 10) / 10),
						t("metricNeededNote"),
					)}
					{rate(
						"metricCost",
						cost ? dollars(cost.totalUsd) : null,
						cost
							? `${cost.byPurpose.map((p) => `${p.words} ${dollars(p.usd)}`).join(", ")}.`
							: undefined,
					)}
					{rate(
						"metricChecks",
						m.mechanicallyVerifiedCriteriaShare === null
							? null
							: `${Math.round(m.mechanicallyVerifiedCriteriaShare * 100)}%`,
						t("metricChecksNote"),
					)}
				</dl>
				<p>{t("metricWeeks").replace("{n}", String(m.activeWeeks))}</p>
				<p>
					{t("metricMessages")
						.replace("{reaction}", String(m.messages.reaction))
						.replace("{reply}", String(m.messages.reply))
						.replace("{ceremony}", String(m.messages.ceremony))
						.replace("{human}", String(m.messages.human))}
				</p>
			</section>
			{limiting && (
				<DailyLimit team={team.team} onClose={() => setLimiting(false)} />
			)}
		</div>
	);
}
