import { Avatar, type AvatarKey, Button, InfoTip, RoleTag } from "@catervas/ui";
import { useState } from "react";
import { useQuery } from "../app/store.ts";
import { Bullets } from "../components/Bullets.tsx";
import { useWide } from "../shell/Shell.tsx";
import { t } from "../strings/t.ts";
import { type AllowanceRow, countSaid, useAllowances } from "./allowances.tsx";
import board from "./Board.module.css";
import own from "./Costs.module.css";
import { DailyLimit } from "./dialogs/DailyLimit.tsx";
import { dollars } from "./Gate.tsx";
import styles from "./pages.module.css";
import type { Team } from "./setup/TeamSetup.tsx";
import type { RoleKit } from "./Team.tsx";

type Summary = {
	todayUsd: number;
	dailyLimitUsd?: number | null;
	agents: { agentId: string; todayUsd: number }[];
	conversationsTodayUsd: number;
};
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

/** Today's spending per agent, and the harness metrics (spec F17). */
export function Costs() {
	const { data: team } = useQuery<{ team: Team; kits?: RoleKit[] }>(
		"team.get",
		{},
	);
	// What agents made on other services; the page draws without it.
	const allowances = useAllowances();
	const wide = useWide();
	const { data: summary } = useQuery<Summary>("costs.summary", {});
	const { data: metrics } = useQuery<Metrics>("metrics", {});
	const [limiting, setLimiting] = useState(false);
	if (!team || !summary || !metrics) return null;
	const today = dollars(summary.todayUsd);

	const rate = (
		key: Parameters<typeof t>[0],
		value: string | null,
		note?: string,
	) => (
		<div key={key} className={own.tile}>
			<dt>
				{t(key)}
				{value !== null && note && (
					<>
						{" "}
						<InfoTip id={`metric-${key}-info`}>{note}</InfoTip>
					</>
				)}
			</dt>
			<dd className={value === null ? undefined : own.value}>
				{value ?? t("metricNotYet")}
			</dd>
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
	const period = t("allowCostsDay");

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
				<Button onClick={() => setLimiting(true)}>{t("costsSetLimit")}</Button>
			</div>
			<section className={styles.section} aria-labelledby="by-agent">
				<div className={styles.titled}>
					<h2 id="by-agent">{t("costsByAgent")}</h2>
					<InfoTip id="resting-info">
						<Bullets text={t("costsResting")} />
					</InfoTip>
				</div>
				<table className={own.table} aria-labelledby="by-agent">
					<thead>
						<tr>
							<th scope="col">{t("costsAgent")}</th>
							<th scope="col" className={own.amount}>
								{t("costsTodayColumn")}
							</th>
						</tr>
					</thead>
					<tbody>
						{[...summary.agents]
							.sort((a, b) => b.todayUsd - a.todayUsd)
							.map((row) => {
								const agent = team.team.agents.find(
									(a) => a.id === row.agentId,
								);
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
												{agent && <RoleTag role={agent.role} />}
											</span>
										</th>
										<td className={own.amount}>{dollars(row.todayUsd)}</td>
									</tr>
								);
							})}
						<tr>
							<th scope="row">
								{t("costsChatsRow")}{" "}
								<InfoTip id="chats-info">{t("costsChatsInfo")}</InfoTip>
							</th>
							<td className={own.amount}>
								{dollars(summary.conversationsTodayUsd)}
							</td>
						</tr>
					</tbody>
				</table>
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
				<p className={styles.muted}>{t("costsWholeProject")}</p>
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
				<details className={own.more}>
					<summary>{t("costsMore")}</summary>
					<p>{t("metricWeeks").replace("{n}", String(m.activeWeeks))}</p>
					<p>
						{t("metricMessages")
							.replace("{reaction}", String(m.messages.reaction))
							.replace("{reply}", String(m.messages.reply))
							.replace("{ceremony}", String(m.messages.ceremony))
							.replace("{human}", String(m.messages.human))}
					</p>
				</details>
			</section>
			{limiting && (
				<DailyLimit team={team.team} onClose={() => setLimiting(false)} />
			)}
		</div>
	);
}
