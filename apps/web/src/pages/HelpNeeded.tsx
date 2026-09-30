import { Button, Choice, TextArea, uiStrings } from "@farik/ui";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import {
	dollars,
	type HistoryEvent,
	signed,
	spent,
	type Tries,
} from "./Gate.tsx";
import own from "./PlanPage.module.css";
import type { Contract } from "./PlanPage.tsx";
import styles from "./pages.module.css";
import { active, sentence } from "./RequestFiled.tsx";
import type { Team } from "./setup/TeamSetup.tsx";

/** One of `escalation.choices`: its label, and the command sent with the message added. */
type EscalationChoice = {
	label: string;
	body: { command: string; body: Record<string, unknown> };
};

/** A request for help: what happened, what was tried, and the choices that fit its reason. */
export function HelpNeeded() {
	const { id = "" } = useParams();
	const { client } = useConnection();
	const navigate = useNavigate();
	const task = { task_id: id };
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { data: plan } = useQuery<{ contract: Contract }>("contract.get", task);
	const { data: history } = useQuery<{ events: HistoryEvent[] }>(
		"task.history",
		task,
	);
	const { data: tries } = useQuery<Tries>("task.tries", task);
	const { data: offered } = useQuery<{ choices: EscalationChoice[] }>(
		"escalation.choices",
		task,
	);
	const [picked, setPicked] = useState("0");
	const [note, setNote] = useState("");
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	if (!team || !plan || !history || !tries || !offered) return null;

	const contract = plan.contract;
	const events = history.events;
	const agents = team.team.agents;
	const builder = agents.find(
		(a) => a.role === contract.assigneeRole && a.status !== "retired",
	);
	const name = builder?.displayName ?? t("you");
	const reviewer =
		contract.reviewerRole && active(agents, contract.reviewerRole);
	const raised = events.findLast((e) => e.kind === "escalation.raised");
	const tried = events.filter(
		(e) => e.kind === "note.written" && e.body.kind === "progress",
	);
	const choices = offered.choices;
	const choice = choices[Number(picked)];

	const resolve = async () => {
		if (!client || !choice) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const reply = await client.command({
				command: choice.body.command,
				body: { ...choice.body.body, message: note.trim() || choice.label },
			} as never);
			if ("error" in reply) setRefusal(sentence(reply.error.detail));
			else navigate("/");
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
				<h1 className={styles.title}>
					{t("helpTitle")
						.replace("{name}", name)
						.replace("{title}", contract.title)}
				</h1>
				<p className={styles.muted}>{id}</p>
			</div>
			{raised && builder && (
				<section className={own.letter} aria-labelledby="explains">
					<p id="explains" className={styles.muted}>
						{signed("helpExplains", builder)}
					</p>
					<p>{raised.body.detail}</p>
				</section>
			)}
			{tried.length > 0 && (
				<section className={styles.section}>
					<h2 id="tried">{t("helpTried").replace("{name}", name)}</h2>
					<ul aria-labelledby="tried">
						{tried.map((e) => (
							<li key={e.seq}>{e.body.text}</li>
						))}
					</ul>
				</section>
			)}
			{choices.length > 0 && (
				<Choice
					name="help-choice"
					legend={t("helpNow")}
					options={choices.map((c, n) => ({
						value: String(n),
						label: c.label,
					}))}
					value={picked}
					onChange={setPicked}
				/>
			)}
			<TextArea
				id="help-note"
				label={t("helpNote").replace("{name}", name)}
				value={note}
				onChange={setNote}
			/>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
			{choice && (
				<div className={styles.actions}>
					<Button kind="primary" busy={busy} onClick={resolve}>
						{choice.label}
					</Button>
				</div>
			)}
			<section className={styles.section} aria-labelledby="about">
				<h2 id="about">{t("gateAbout")}</h2>
				<dl className={own.facts}>
					<dt>{t("helpWaiting")}</dt>
					<dd>
						{raised ? `${raised.recordedAt.slice(11, 16)} UTC` : t("notYet")}
					</dd>
					<dt>{t("gateTries")}</dt>
					<dd>
						{t("triesOf")
							.replace("{used}", String(tries.used))
							.replace("{allowed}", String(tries.allowed))}
					</dd>
					<dt>{t("helpSpent")}</dt>
					<dd>
						{t("helpSpentOf")
							.replace("{spent}", dollars(spent(events)))
							.replace("{max}", dollars(contract.budget.maxCostUsd))}
					</dd>
					<dt>{t("helpReviewer")}</dt>
					<dd>
						{reviewer
							? t("nameRole")
									.replace("{name}", reviewer.displayName)
									.replace("{role}", uiStrings.roleName[reviewer.role])
							: t("notYet")}
					</dd>
				</dl>
			</section>
		</div>
	);
}
