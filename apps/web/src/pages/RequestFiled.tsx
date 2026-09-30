import { Button, uiStrings } from "@farik/ui";
import { useState } from "react";
import { Link, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import {
	active,
	type EscalationReason,
	sentence,
	statusWord,
	type TaskStatus,
} from "../app/words.ts";
import { t } from "../strings/t.ts";
import { Failed } from "./Failed.tsx";
import styles from "./pages.module.css";
import own from "./RequestFiled.module.css";
import type { Agent, Team } from "./setup/TeamSetup.tsx";

type Size = "large" | "small";
type HistoryEvent = {
	recordedAt: string;
	kind: string;
	body: { size?: Size; reason?: string; triagedBy?: string };
};

/** A request's page: what was asked, how it was sized, and the other size one click away. */
export function RequestFiled() {
	const { id = "" } = useParams();
	const { client } = useConnection();
	const { data: team, error: e1 } = useQuery<{ team: Team }>("team.get", {});
	const { data: contract, error: e2 } = useQuery<{
		contract: { intent: string; status: TaskStatus };
	}>("contract.get", { task_id: id });
	const { data: history, error: e3 } = useQuery<{ events: HistoryEvent[] }>(
		"task.history",
		{ task_id: id },
	);
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const failed = e1 ?? e2 ?? e3;
	// A read refused before the page has what it needs: say why (a task that is not there).
	if (!team || !contract || !history)
		return failed ? <Failed error={failed} /> : null;

	const agents = team.team.agents;
	const pm = active(agents, "product_manager");
	// SPEC 5.16: the Scrum Master sizes and breaks down, or the PM when there is none.
	const sizer = active(agents, "scrum_master") ?? pm;
	const pmName = pm?.displayName ?? uiStrings.roleName.product_manager;
	const sizerName = sizer?.displayName ?? pmName;
	const created = history.events.find((e) => e.kind === "task.created");
	const triage = history.events.findLast((e) => e.kind === "request.triaged");
	const size = triage?.body.size;
	const by = agents.find((a) => a.id === triage?.body.triagedBy);
	const byHuman = triage?.body.triagedBy === "human";
	const byName = byHuman ? t("you") : (by?.displayName ?? sizerName);
	const nameRole = (a: Agent | undefined) =>
		a
			? t("nameRole", { name: a.displayName, role: uiStrings.roleName[a.role] })
			: t("notYet");
	const names = { pm: pmName, breaker: sizerName };
	const chosen = byHuman
		? t("sizeChosenByYou")
		: t("sizeChosen", { name: byName });

	const resize = async (to: Size) => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const reply = await client.command({
				command: "request_triage",
				body: { taskId: id, size: to, reason: t("resizeReason") },
			});
			if ("error" in reply) setRefusal(sentence(reply.error.detail));
		} catch {
			// The connection closed: the page shows that it is lost.
		} finally {
			setBusy(false);
		}
	};

	return (
		<div className={styles.page}>
			<Link to="/">{t("backToToday")}</Link>
			<div className={styles.section}>
				<h1 className={styles.title}>{t("requestTitle")}</h1>
				{created && (
					<p className={styles.muted}>
						{t("requestSent", { id, time: created.recordedAt.slice(11, 16) })}
					</p>
				)}
			</div>
			<section className={styles.section} aria-labelledby="asked">
				<h2 id="asked">{t("youAsked")}</h2>
				<p className={own.asked}>{contract.contract.intent}</p>
			</section>
			{size ? (
				<section className={styles.section} aria-labelledby="sized">
					<h2 id="sized">
						{t(byHuman ? "youSizedAs" : "sizedAs", {
							name: byName,
							size: t(size === "large" ? "sizeLarge" : "sizeSmall"),
						})}
					</h2>
					{by && (
						<p className={styles.muted}>
							{t("decided", {
								name: by.displayName,
								role: uiStrings.roleName[by.role],
							})}
						</p>
					)}
					<p>{triage.body.reason}</p>
					<div className={own.cards}>
						{(["large", "small"] as const).map((one) => (
							<div key={one} className={one === size ? own.chosen : own.card}>
								<h3>
									{t(one === "large" ? "sizeLargeCard" : "sizeSmallCard")}
								</h3>
								<p>
									{t(
										one === "large" ? "sizeLargeBody" : "sizeSmallBody",
										names,
									)}
								</p>
								{one === size && <p>{chosen}</p>}
							</div>
						))}
					</div>
					{/* The governor takes a human triage only while the task is a draft. */}
					{contract.contract.status === "draft" && (
						<div>
							<Button
								busy={busy}
								onClick={() => resize(size === "large" ? "small" : "large")}
							>
								{t(size === "large" ? "resizeToSmall" : "resizeToLarge")}
							</Button>
						</div>
					)}
					{refusal && (
						<p role="alert" className={own.alert}>
							{refusal}
						</p>
					)}
				</section>
			) : (
				<p>{t("sizing", { name: sizerName })}</p>
			)}
			{size && (
				<section className={styles.section} aria-labelledby="next">
					<h2 id="next">{t("nextTitle")}</h2>
					<ol aria-labelledby="next">
						{([1, 2, 3, 4] as const).map((n) => (
							<li key={n}>
								{t(
									`next${size === "large" ? "Large" : "Small"}${n}` as const,
									names,
								)}
							</li>
						))}
					</ol>
				</section>
			)}
			<section className={styles.section} aria-labelledby="about">
				<h2 id="about">{t("aboutRequest")}</h2>
				<dl className={own.facts}>
					<dt>{t("aboutSent")}</dt>
					<dd>
						{created
							? new Date(created.recordedAt).toLocaleDateString("en-GB", {
									weekday: "long",
									day: "numeric",
									month: "long",
									timeZone: "UTC",
								})
							: t("notYet")}
					</dd>
					<dt>{t("aboutSizedBy")}</dt>
					<dd>{byHuman ? t("you") : nameRole(size ? by : undefined)}</dd>
					<dt>{t("aboutPlannedBy")}</dt>
					<dd>{nameRole(pm)}</dd>
					<dt>{t("aboutPlanId")}</dt>
					<dd>{id}</dd>
					<dt>{t("aboutStatus")}</dt>
					<dd>
						{statusWord(
							contract.contract.status,
							history.events.findLast((e) => e.kind === "escalation.raised")
								?.body.reason as EscalationReason | undefined,
						)}
					</dd>
				</dl>
			</section>
		</div>
	);
}
