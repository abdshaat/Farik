import { Button, DiffView, StatusWord } from "@farik/ui";
import { type KeyboardEvent, type ReactNode, useState } from "react";
import { Link, useParams } from "react-router";
import { useQuery } from "../app/store.ts";
import { statusWord } from "../app/words.ts";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import { CancelTask } from "./dialogs/CancelTask.tsx";
import { useCommand } from "./dialogs/StartSprint.tsx";
import {
	dollars,
	type HistoryEvent,
	latestNote,
	signed,
	type Tries,
} from "./Gate.tsx";
import own from "./PlanPage.module.css";
import { type Contract, day, riskWord } from "./PlanPage.tsx";
import styles from "./pages.module.css";
import type { Team } from "./setup/TeamSetup.tsx";
import page from "./TaskDetail.module.css";

/** The contract fields this page reads beyond the plan pages'. */
type Detail = Contract & {
	sprint?: string;
	assignee?: string;
	reviewer?: string;
	change?: "feature" | "fix";
	notes?: { completion?: string; review?: string; escalation?: string };
};
type Event = HistoryEvent & {
	agentId?: string;
	body: HistoryEvent["body"] & { createdBy?: string; requestedBy?: string };
};
type Check = { criterionId: string; passed: boolean };
type Diff = { diff: string; files: string[]; added: number; removed: number };
type Waiting = { taskId: string; kind: string };
type Activity = {
	agentId: string;
	state: string;
	taskId?: string;
	sessionId?: string;
};
type Costs = {
	byPurpose: { words: string; usd: number }[];
	totalUsd: number;
	limitUsd: number;
};

const TABS = ["summary", "history", "plan", "changes", "notes"] as const;
type Tab = (typeof TABS)[number];
const TAB_WORDS: Record<Tab, keyof typeof en> = {
	summary: "tabSummary",
	history: "tabHistory",
	plan: "tabPlan",
	changes: "tabChanges",
	notes: "tabNotes",
};

/** Each history kind's plain words, `{who}` its actor (step 07's `moved.since` phrasing). */
const TOLD: Record<string, keyof typeof en> = {
	"task.created": "toldCreated",
	"task.transitioned": "toldMoved",
	"note.written": "toldNote",
	"review.recorded": "toldReview",
	"criterion.recorded": "toldCheck",
	"session.started": "toldStarted",
	"session.ended": "toldEnded",
	"cost.recorded": "toldCost",
	"escalation.raised": "toldEscalated",
	"escalation.resolved": "toldResolved",
	"task.integrated": "toldIntegrated",
	"contract.written": "toldPlan",
	"contract.evaluated": "toldPlan",
	"contract.judged": "toldPlan",
};

const NOTES = [
	["completion", "gateWrote"],
	["review", "gateReviewed"],
	["escalation", "helpExplains"],
] as const;

/** One task: its five tabs, what it cost, and what the user can do with it (step 09's plan). */
export function TaskDetail() {
	const { id = "" } = useParams();
	const task = { task_id: id };
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { data: plan } = useQuery<{ contract: Detail }>("contract.get", task);
	const { data: history } = useQuery<{ events: Event[] }>("task.history", task);
	const { data: checked } = useQuery<{ checks: Check[] }>("task.checks", task);
	const { data: diff } = useQuery<Diff>("task.diff", task);
	const { data: tries } = useQuery<Tries>("task.tries", task);
	const { data: waiting } = useQuery<{ waiting: Waiting[] }>(
		"waiting.list",
		{},
	);
	const { data: activity } = useQuery<{ activity: Activity[] }>(
		"team.activity",
		{},
	);
	const { data: costs } = useQuery<Costs>("task.costs", task);
	const [tab, setTab] = useState<Tab>("summary");
	const [cancelling, setCancelling] = useState(false);
	const { busy, refusal, send } = useCommand(() => {});
	if (
		!team ||
		!plan ||
		!history ||
		!checked ||
		!diff ||
		!tries ||
		!waiting ||
		!activity ||
		!costs
	)
		return null;

	const contract = plan.contract;
	const events = history.events;
	const agents = team.team.agents;
	const agentOf = (who?: string) =>
		agents.find((a) => a.id === who || a.displayName === who);
	const nameOf = (who?: string) =>
		who === "human"
			? t("you")
			: who === "governor" || who === "farik" || !who
				? "Farik"
				: (agentOf(who)?.displayName ?? who);

	const assignee = contract.assignee && nameOf(contract.assignee);
	const reviewer = contract.reviewer && nameOf(contract.reviewer);
	const doing = assignee && t("taskDoing").replace("{name}", assignee);
	const reviews = reviewer && t("taskReviews").replace("{name}", reviewer);
	const who = [doing, reviews].filter(Boolean).join(", and ");
	const lead = [
		contract.sprint
			? t("taskInSprint")
					.replace("{id}", id)
					.replace("{n}", contract.sprint.slice(1))
			: `${id}.`,
		who && `${who}.`,
		t("taskTry")
			.replace("{try}", String(tries.try))
			.replace("{of}", String(tries.of)),
	]
		.filter(Boolean)
		.join(" ");

	const summary = events.findLast(
		(e) =>
			e.kind === "note.written" &&
			(e.body.kind === "completion" || e.body.kind === "review"),
	);
	const summaryBy = agentOf(summary?.body.writtenBy);
	const approved = events.find(
		(e) =>
			(e.kind === "human.accepted" && e.body.subject === "contract") ||
			(e.kind === "task.transitioned" && e.body.to === "ready"),
	);
	const integrated = events.findLast((e) => e.kind === "task.integrated");
	const awaitsIntegration = waiting.waiting.some(
		(w) => w.taskId === id && w.kind === "integration",
	);
	const session = activity.activity.find(
		(a) => a.state === "working" && a.taskId === id && a.sessionId,
	)?.sessionId;
	const cancellable =
		contract.status !== "accepted" && contract.status !== "cancelled";
	// The branch as `farik_core::branch::task_branch` names it.
	const branch =
		contract.assigneeRole === "software_developer"
			? `${contract.change === "fix" ? "fix" : "feature"}/${id}`
			: `docs/${id}`;

	const told = (e: Event) => {
		const key =
			e.kind === "human.accepted"
				? e.body.subject === "contract"
					? "toldApproved"
					: "toldAccepted"
				: (TOLD[e.kind] ?? "toldOther");
		const actor =
			e.body.requestedBy ??
			e.body.createdBy ??
			(e.kind.startsWith("human.") ? "human" : e.agentId);
		return t(key)
			.replace("{who}", nameOf(actor))
			.replace(
				"{status}",
				e.body.to ? statusWord(e.body.to as Contract["status"]) : "",
			)
			.replace("{usd}", dollars(e.body.costUsd ?? 0));
	};

	const onKey = (e: KeyboardEvent) => {
		const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
		if (!step) return;
		const next =
			TABS[(TABS.indexOf(tab) + step + TABS.length) % TABS.length] ?? tab;
		setTab(next);
		document.getElementById(`tab-${next}`)?.focus();
	};

	const panels: Record<Tab, ReactNode> = {
		summary: (
			<>
				<h2>{t("taskFor")}</h2>
				<p>{contract.intent}</p>
				{summary && summaryBy && (
					<section className={own.letter} aria-labelledby="summary-signed">
						<p id="summary-signed" className={styles.muted}>
							{signed(
								summary.body.kind === "review" ? "gateReviewed" : "gateWrote",
								summaryBy,
							)}
						</p>
						<p>{summary.body.text?.split(/\n\s*\n/)[0]}</p>
					</section>
				)}
				<h2 id="checks">{t("taskChecks")}</h2>
				<ul className={own.checks} aria-labelledby="checks">
					{contract.exitCriteria.map((c) => {
						const check = checked.checks.find((k) => k.criterionId === c.id);
						return (
							<li key={c.id}>
								<span>{c.text}</span>
								<span className={styles.muted}>
									{t(
										!check
											? "checkNotRun"
											: check.passed
												? "checkPassed"
												: "checkFailed",
									)}
								</span>
							</li>
						);
					})}
				</ul>
			</>
		),
		history: (
			<>
				<h2>{t("tabHistory")}</h2>
				<p className={styles.muted}>{t("taskHistoryHint")}</p>
				<ol className={page.history}>
					{[...events].reverse().map((e) => (
						<li key={e.seq}>
							<time dateTime={e.recordedAt}>
								{`${day(e.recordedAt)}, ${e.recordedAt.slice(11, 16)}`}
							</time>
							<span>{told(e)}</span>
							<span className={page.kind}>{e.kind}</span>
						</li>
					))}
				</ol>
			</>
		),
		plan: (
			<>
				<h2>{t("tabPlan")}</h2>
				<p>
					{approved
						? t("taskPlanApproved").replace("{day}", day(approved.recordedAt))
						: t("taskPlanNotApproved")}
				</p>
				<p>{t(contract.locked ? "taskPlanLocked" : "taskPlanOpen")}</p>
				<dl className={own.facts}>
					<dt>{t("taskScope")}</dt>
					<dd>
						<ul className={page.plain}>
							{contract.scope.inScope.map((line) => (
								<li key={line}>{line}</li>
							))}
						</ul>
					</dd>
					<dt>{t("taskOutOfScope")}</dt>
					<dd>
						<ul className={page.plain}>
							{contract.scope.outOfScope.map((line) => (
								<li key={line}>{line}</li>
							))}
						</ul>
					</dd>
					<dt>{t("taskRisk")}</dt>
					<dd>{riskWord(contract.risk)}</dd>
					<dt>{t("taskLimit")}</dt>
					<dd>
						{t("taskLimitFor").replace(
							"{usd}",
							dollars(contract.budget.maxCostUsd),
						)}
					</dd>
				</dl>
			</>
		),
		changes: (
			<>
				<h2>{t("tabChanges")}</h2>
				<p>
					{t("taskChangesLine")
						.replace(
							"{size}",
							t(diff.files.length === 1 ? "gateSizeOne" : "gateSize")
								.replace("{n}", String(diff.files.length))
								.replace("{added}", String(diff.added))
								.replace("{removed}", String(diff.removed)),
						)
						.replace("{branch}", branch)}
				</p>
				<DiffView diff={diff.diff} label={t("gateChanges")} />
			</>
		),
		notes: (
			<>
				<h2>{t("tabNotes")}</h2>
				{NOTES.some(([kind]) => contract.notes?.[kind]) ? (
					NOTES.map(([kind, key]) => {
						const text = contract.notes?.[kind];
						const by = agentOf(latestNote(events, kind)?.body.writtenBy);
						if (!text) return null;
						return (
							<div key={kind} className={own.letter}>
								{by && <p className={styles.muted}>{signed(key, by)}</p>}
								<p>{text}</p>
							</div>
						);
					})
				) : (
					<p className={styles.muted}>{t("taskNoNotes")}</p>
				)}
			</>
		),
	};

	return (
		<div className={page.layout}>
			<div className={page.main}>
				<Link to="/board">{t("taskBack")}</Link>
				<div className={styles.section}>
					<h1 className={styles.title}>{contract.title}</h1>
					<StatusWord
						tone={
							contract.status === "accepted" || contract.status === "cancelled"
								? "done"
								: contract.status === "escalated" ||
										contract.status === "verifying"
									? "waiting"
									: "working"
						}
					>
						{statusWord(contract.status)}
					</StatusWord>
					<p className={styles.muted}>{lead}</p>
				</div>
				<div
					role="tablist"
					aria-label={t("taskTabs")}
					className={page.tabs}
					onKeyDown={onKey}
				>
					{TABS.map((one) => (
						<button
							key={one}
							id={`tab-${one}`}
							type="button"
							role="tab"
							aria-selected={one === tab}
							aria-controls="task-panel"
							tabIndex={one === tab ? 0 : -1}
							className={page.tab}
							onClick={() => setTab(one)}
						>
							{t(TAB_WORDS[one])}
						</button>
					))}
				</div>
				<div
					id="task-panel"
					role="tabpanel"
					aria-labelledby={`tab-${tab}`}
					className={`${styles.section} ${page.panel}`}
				>
					{panels[tab]}
				</div>
			</div>
			<aside className={page.side}>
				<section className={styles.section} aria-labelledby="cost">
					<h2 id="cost">{t("taskCost")}</h2>
					<table className={page.cost}>
						<tbody>
							{costs.byPurpose.map((row) => (
								<tr key={row.words}>
									<th scope="row">{row.words}</th>
									<td>{dollars(row.usd)}</td>
								</tr>
							))}
							<tr className={page.total}>
								<th scope="row">
									{t("taskCostTotal").replace(
										"{limit}",
										dollars(costs.limitUsd),
									)}
								</th>
								<td>{dollars(costs.totalUsd)}</td>
							</tr>
						</tbody>
					</table>
				</section>
				<section className={styles.section} aria-labelledby="adding">
					<h2 id="adding">{t("taskAddTitle")}</h2>
					<p className={styles.muted}>{t("taskAddHint")}</p>
					{integrated ? (
						<p>{t("taskAdded").replace("{day}", day(integrated.recordedAt))}</p>
					) : awaitsIntegration ? (
						<div className={styles.actions}>
							<Button
								kind="primary"
								busy={busy}
								onClick={() =>
									send({ command: "task_integrate", body: { taskId: id } })
								}
							>
								{t("taskAdd")}
							</Button>
						</div>
					) : (
						<p>{t("taskAddNotYet")}</p>
					)}
				</section>
				{(session || cancellable) && (
					<div className={page.stack}>
						{session && (
							<Button
								busy={busy}
								onClick={() =>
									send({
										command: "session_stop",
										body: { sessionId: session },
									})
								}
							>
								{t("taskStop")}
							</Button>
						)}
						{cancellable && (
							<Button onClick={() => setCancelling(true)}>
								{t("taskCancel")}
							</Button>
						)}
					</div>
				)}
				{refusal && (
					<p role="alert" className={styles.alert}>
						{refusal}
					</p>
				)}
			</aside>
			{cancelling && (
				<CancelTask
					id={id}
					title={contract.title}
					escalated={contract.status === "escalated"}
					onClose={() => setCancelling(false)}
				/>
			)}
		</div>
	);
}
