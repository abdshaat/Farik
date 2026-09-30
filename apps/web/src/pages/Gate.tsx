import { Button, DiffView, TextArea, uiStrings } from "@farik/ui";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import { sentence, statusWord } from "../app/words.ts";
import { t } from "../strings/t.ts";
import gate from "./Gate.module.css";
import own from "./PlanPage.module.css";
import { type Contract, day } from "./PlanPage.tsx";
import styles from "./pages.module.css";
import { SendBackDialog } from "./SendBackDialog.tsx";
import type { Agent, Team } from "./setup/TeamSetup.tsx";

/** One event of `task.history`, with the body fields these pages read. */
export type HistoryEvent = {
	seq: number;
	recordedAt: string;
	kind: string;
	agentId?: string;
	body: {
		kind?: string;
		text?: string;
		writtenBy?: string;
		subject?: string;
		to?: string;
		costUsd?: number;
		detail?: string;
		reason?: string;
	};
};
/** The try in progress, of all the task gets, as `task.tries` works them out. */
export type Tries = { try: number; of: number };
type Check = { criterionId: string; text: string; passed: boolean };
type Diff = { diff: string; files: string[]; added: number; removed: number };
type Waiting = { taskId: string; kind: string };

export const dollars = (n: number) => `$${n.toFixed(2)}`;
/** What the task's sessions cost, from its `cost.recorded` events. */
export const spent = (events: HistoryEvent[]) =>
	events.reduce(
		(sum, e) => sum + (e.kind === "cost.recorded" ? (e.body.costUsd ?? 0) : 0),
		0,
	);
/** The latest note of `kind`. */
export const latestNote = (events: HistoryEvent[], kind: string) =>
	events.findLast((e) => e.kind === "note.written" && e.body.kind === kind);
/** "<name>, your <role>, …" for the agent `id`. */
export const signed = (
	key: "gateWrote" | "gateReviewed" | "helpExplains",
	agent: Agent,
) => t(key, { name: agent.displayName, role: uiStrings.roleName[agent.role] });

/** The acceptance gate: the two summaries, Farik's checks, and the code one click away. */
export function Gate() {
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
	const { data: checked } = useQuery<{ checks: Check[] }>("task.checks", task);
	const { data: diff } = useQuery<Diff>("task.diff", task);
	const { data: tries } = useQuery<Tries>("task.tries", task);
	const { data: waiting } = useQuery<{ waiting: Waiting[] }>(
		"waiting.list",
		{},
	);
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const [showDiff, setShowDiff] = useState(false);
	const [sending, setSending] = useState(false);
	const [what, setWhat] = useState("");
	if (!team || !plan || !history || !checked || !diff || !tries || !waiting)
		return null;

	const contract = plan.contract;
	const events = history.events;
	const agents = team.team.agents;
	const epic = contract.kind === "epic";
	const waits = (kind: string) =>
		waiting.waiting.some((w) => w.taskId === id && w.kind === kind);
	const integration = waits("integration");
	const builder = agents.find(
		(a) => a.role === contract.assigneeRole && a.status !== "retired",
	);
	const approved = events.find(
		(e) =>
			(e.kind === "human.accepted" && e.body.subject === "contract") ||
			(e.kind === "task.transitioned" && e.body.to === "ready"),
	);
	const letters = [
		["completion", "gateWrote"],
		["review", "gateReviewed"],
	] as const;

	const send = async (command: object) => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const reply = await client.command(command as never);
			if ("error" in reply) setRefusal(sentence(reply.error.detail));
			else navigate("/");
		} catch {
			// The connection closed: the page shows that it is lost.
		} finally {
			setBusy(false);
			setSending(false);
		}
	};

	return (
		<div className={styles.page}>
			<Link to="/">{t("backToToday")}</Link>
			<div className={styles.section}>
				<h1 className={styles.title}>
					{t("gateTitle", { title: contract.title })}
				</h1>
				<p className={styles.muted}>{id}</p>
			</div>
			{letters.map(([kind, key]) => {
				const note = latestNote(events, kind);
				const by = agents.find((a) => a.id === note?.body.writtenBy);
				if (!note || !by) return null;
				return (
					<section
						key={kind}
						className={own.letter}
						aria-labelledby={`${kind}-signed`}
					>
						<p id={`${kind}-signed`} className={styles.muted}>
							{signed(key, by)}
						</p>
						<p>{note.body.text?.split(/\n\s*\n/)[0]}</p>
					</section>
				);
			})}
			<section className={styles.section} aria-labelledby="checked">
				<h2 id="checked">{t("planChecked")}</h2>
				<p className={styles.muted}>{t("gateCheckedHint")}</p>
				<ul className={own.checks}>
					{checked.checks.map((c) => (
						<li key={c.criterionId}>
							<span>{c.text}</span>
							<span className={styles.muted}>
								{t(c.passed ? "checkPassed" : "checkNotYet")}
							</span>
						</li>
					))}
				</ul>
			</section>
			<div className={styles.section}>
				<button
					type="button"
					className={gate.toggle}
					aria-expanded={showDiff}
					onClick={() => setShowDiff((open) => !open)}
				>
					{`${t(showDiff ? "gateHideChanges" : "gateSeeChanges")} · ${t(diff.files.length === 1 ? "gateSizeOne" : "gateSize", { n: String(diff.files.length), added: String(diff.added), removed: String(diff.removed) })}`}
				</button>
				{showDiff && <DiffView diff={diff.diff} label={t("gateChanges")} />}
			</div>
			<section className={styles.section} aria-labelledby="about">
				<h2 id="about">{t("gateAbout")}</h2>
				<dl className={own.facts}>
					<dt>{t("youAsked")}</dt>
					<dd>{day(contract.createdAt)}</dd>
					<dt>{t("gatePlanApproved")}</dt>
					<dd>{day(approved?.recordedAt)}</dd>
					<dt>{t("gateTries")}</dt>
					<dd>
						{t("triesOf", { try: String(tries.try), of: String(tries.of) })}
					</dd>
					<dt>{t("gateCost")}</dt>
					<dd>{dollars(spent(events))}</dd>
				</dl>
				{/* ponytail: step 09's task detail replaces this list. */}
				<details className={own.written}>
					<summary>{t("gateHistory")}</summary>
					<ol>
						{events.map((e) => (
							<li key={e.seq}>
								{e.recordedAt.slice(11, 16)} {e.kind}
							</li>
						))}
					</ol>
				</details>
			</section>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
			{integration ? (
				<div className={styles.actions}>
					<Button
						kind="primary"
						busy={busy}
						onClick={() =>
							send({ command: "task_integrate", body: { taskId: id } })
						}
					>
						{t("gateAdd")}
					</Button>
				</div>
			) : !waits("acceptance") ? (
				<p className={styles.muted}>
					{t("notWaiting", { status: statusWord(contract.status) })}
				</p>
			) : (
				<>
					{epic && (
						<TextArea
							id="what-you-checked"
							label={t("gateWhatChecked")}
							value={what}
							onChange={setWhat}
							required
						/>
					)}
					<div className={styles.actions}>
						<Button
							kind="primary"
							busy={busy}
							disabled={epic && what.trim() === ""}
							onClick={() =>
								send({
									command: "human_accept",
									body: {
										taskId: id,
										subject: "result",
										...(epic ? { message: what.trim() } : {}),
									},
								})
							}
						>
							{t("gateAccept")}
						</Button>
						<Button busy={busy} onClick={() => setSending(true)}>
							{t("gateSendBack")}
						</Button>
					</div>
				</>
			)}
			<SendBackDialog
				open={sending}
				title={contract.title}
				builder={builder?.displayName ?? t("you")}
				criteria={contract.exitCriteria}
				tries={tries}
				busy={busy}
				onClose={() => setSending(false)}
				onSend={(failedCriteria, message) =>
					send({
						command: "human_send_back",
						body: { taskId: id, subject: "result", message, failedCriteria },
					})
				}
			/>
		</div>
	);
}
