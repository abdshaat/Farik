import { Button, Choice, TextArea, uiStrings } from "@farik/ui";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import { active, sentence } from "../app/words.ts";
import { t } from "../strings/t.ts";
import { Failed } from "./Failed.tsx";
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
import type { Team } from "./setup/TeamSetup.tsx";

/** One of `escalation.choices`: its label, and the command sent with the message added. */
type EscalationChoice = {
	label: string;
	body: { command: string; body: Record<string, unknown> };
};

/** The reasons the governor raises in its own words, not an agent's (SPEC 5.7). */
const FARIKS = ["iterations", "budget", "sessions", "blocker_age"];

/** A request for help: what happened, what was tried, and the choices that fit its reason. */
export function HelpNeeded() {
	const { id = "" } = useParams();
	const { client } = useConnection();
	const navigate = useNavigate();
	const task = { task_id: id };
	const { data: team, error: e1 } = useQuery<{ team: Team }>("team.get", {});
	const { data: plan, error: e2 } = useQuery<{ contract: Contract }>(
		"contract.get",
		task,
	);
	const { data: history, error: e3 } = useQuery<{ events: HistoryEvent[] }>(
		"task.history",
		task,
	);
	const { data: tries, error: e4 } = useQuery<Tries>("task.tries", task);
	const { data: offered, error: e5 } = useQuery<{
		choices: EscalationChoice[];
	}>("escalation.choices", task);
	const [picked, setPicked] = useState("0");
	const [note, setNote] = useState("");
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const failed = e1 ?? e2 ?? e3 ?? e4 ?? e5;
	// A read refused before the page has what it needs: say why (a task that is not there).
	if (!team || !plan || !history || !tries || !offered)
		return failed ? <Failed error={failed} /> : null;

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
	const fariks = !raised?.agentId || FARIKS.includes(raised.body.reason ?? "");
	// What was tried since the human last answered an escalation of this task.
	const since =
		events.findLast((e) => e.kind === "escalation.resolved")?.seq ?? 0;
	const tried = events.filter(
		(e) =>
			e.seq > since && e.kind === "note.written" && e.body.kind === "progress",
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
					{t("helpTitle", { name, title: contract.title })}
				</h1>
				<p className={styles.muted}>{id}</p>
			</div>
			{raised && (
				<section className={own.letter} aria-labelledby="explains">
					<p id="explains" className={styles.muted}>
						{fariks || !builder
							? t("helpFarikExplains")
							: signed("helpExplains", builder)}
					</p>
					<p>{raised.body.detail}</p>
				</section>
			)}
			{tried.length > 0 && (
				<section className={styles.section}>
					<h2 id="tried">{t("helpTried", { name })}</h2>
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
			{choices.length > 0 ? (
				<TextArea
					id="help-note"
					label={t("helpNote", { name })}
					value={note}
					onChange={setNote}
				/>
			) : raised?.body.reason === "approval" ? (
				<p>
					<Link to={`/tasks/${id}/plan`}>{t("helpReadPlan")}</Link>
				</p>
			) : (
				<p className={styles.muted}>{t("helpNothing")}</p>
			)}
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
						{t("triesOf", { try: String(tries.try), of: String(tries.of) })}
					</dd>
					<dt>{t("helpSpent")}</dt>
					<dd>
						{t("helpSpentOf", {
							spent: dollars(spent(events)),
							max: dollars(contract.budget.maxCostUsd),
						})}
					</dd>
					<dt>{t("helpReviewer")}</dt>
					<dd>
						{reviewer
							? t("nameRole", {
									name: reviewer.displayName,
									role: uiStrings.roleName[reviewer.role],
								})
							: t("notYet")}
					</dd>
				</dl>
			</section>
		</div>
	);
}
