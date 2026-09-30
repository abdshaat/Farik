import { Button, DiffView, StatusWord } from "@farik/ui";
import { type KeyboardEvent, type ReactNode, useState } from "react";
import { Link, useParams } from "react-router";
import { useQuery } from "../app/store.ts";
import { movedWords, statusWord } from "../app/words.ts";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import { DesignReview, type Recorded, type Review } from "./DesignReview.tsx";
import { CancelTask } from "./dialogs/CancelTask.tsx";
import { useCommand } from "./dialogs/StartSprint.tsx";
import {
	approvesPlan,
	dollars,
	type HistoryEvent,
	latestNote,
	signed,
	type Tries,
} from "./Gate.tsx";
import own from "./PlanPage.module.css";
import { type Contract, day } from "./PlanPage.tsx";
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
	body: HistoryEvent["body"] & Partial<Record<(typeof ACTORS)[number], string>>;
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
/** The Designer's latest plan and the Product Manager's decision on it, as `task.get` answers it. */
type DesignPlan = {
	plan: string;
	state: "proposed" | "approved" | "returned";
	reason?: string;
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
	"contract.locked": "toldLocked",
	"contract.unlocked": "toldUnlocked",
	"question.asked": "toldAsked",
	"question.answered": "toldAnswered",
	"request.triaged": "toldTriaged",
	"budget.exhausted": "toldLimit",
	"transition.refused": "toldRefused",
	"escalation.aged": "toldAged",
	"pull_request.opened": "toldPullRequest",
	"sprint.planned": "toldPlanned",
	"drift.detected": "toldDrift",
	"message.posted": "toldSaid",
	"tool.denied": "toldDenied",
	"design_plan.proposed": "toldDesignProposed",
	"design_plan.approved": "toldDesignApproved",
	"design_plan.returned": "toldDesignReturned",
};

/** The body fields that name who did it, one per kind that has one. */
const ACTORS = [
	"requestedBy",
	"createdBy",
	"lockedBy",
	"unlockedBy",
	"askedBy",
	"answeredBy",
	"triagedBy",
	"plannedBy",
	"author",
] as const;

/** Ordinals for the plan count and the returns limit, as the approved mockup words them. */
const NTH = [
	"first",
	"second",
	"third",
	"fourth",
	"fifth",
	"sixth",
	"seventh",
	"eighth",
	"ninth",
	"tenth",
];

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
	const { data: detail } = useQuery<{
		designPlan: DesignPlan | null;
		designReview?: Review | null;
		designReviews?: Recorded[];
	}>("task.get", task);
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
		!costs ||
		!detail
	)
		return null;

	const contract = plan.contract;
	const events = history.events;
	const agents = team.team.agents;
	const agentOf = (who?: string) =>
		agents.find((a) => a.id === who || a.displayName === who);
	// Agents write notes as log events; the contract's own notes are hand-written ones.
	const noteOf = (kind: (typeof NOTES)[number][0]) =>
		latestNote(events, kind)?.body.text ?? contract.notes?.[kind];
	const nameOf = (who?: string) =>
		who === "human"
			? t("you")
			: who === "governor" || who === "farik" || !who
				? "Farik"
				: (agentOf(who)?.displayName ?? who);

	// A Designer's task in progress counts its plans in place of its tries (the approved mockup).
	const plans =
		detail.designPlan && contract.status === "in_progress"
			? events.filter((e) => e.kind === "design_plan.proposed").length
			: 0;
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
		plans
			? plans <= NTH.length
				? t("designPlanCount", { Nth: capital(NTH[plans - 1] ?? "") })
				: t("designPlanCountMany", { n: plans })
			: t("taskTry")
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
	const approved = events.find(approvesPlan);
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
		contract.assigneeRole === "software_developer" ||
		contract.assigneeRole === "ui_ux_designer"
			? `${contract.change === "fix" ? "fix" : "feature"}/${id}`
			: `docs/${id}`;

	const design = detail.designPlan;
	const proposal = events.findLast((e) => e.kind === "design_plan.proposed");
	// The decision on the latest plan, never one on a plan before it.
	const decision = events.findLast(
		(e) =>
			(e.kind === "design_plan.approved" ||
				e.kind === "design_plan.returned") &&
			e.seq > (proposal?.seq ?? 0),
	);
	const designer = nameOf(proposal?.agentId ?? contract.assignee);
	const pm = nameOf(
		decision?.agentId ??
			agents.find((a) => a.role === "product_manager" && a.status === "active")
				?.id,
	);
	// "today", or "on Wednesday 30 September", in UTC as the times are.
	const when = (e?: Event) =>
		e
			? {
					day:
						e.recordedAt.slice(0, 10) === new Date().toISOString().slice(0, 10)
							? t("designToday")
							: t("designOnDay", { day: day(e.recordedAt) }),
					time: e.recordedAt.slice(11, 16),
				}
			: {};
	const returns = events.filter(
		(e) => e.kind === "design_plan.returned",
	).length;
	const limit = tries.of - 1;
	// The plan's state stands for the task's while the Designer's task is being worked on.
	const review = detail.designReview;
	const checking = review?.state === "waiting";
	const onDesigner = review?.state === "waiting_on_designer";
	const noBrowser = review?.state === "designer_needs_browser";
	const designWord =
		(design &&
			contract.status === "in_progress" &&
			(design.state === "proposed"
				? t("designWaiting", { pm })
				: t(
						design.state === "approved" ? "designBeingBuilt" : "designSentBack",
					))) ||
		(checking && t("designChecking")) ||
		(onDesigner && t("designOnDesigner")) ||
		(noBrowser && t("designNoBrowser"));
	// The team's Designer, who checks every screen a Developer changes: the active one, as the
	// runtime picks it, else a paused one the review waits on.
	const designers = agents.filter(
		(a) => a.role === "ui_ux_designer" && a.status !== "retired",
	);
	const iris = designers.find((a) => a.status === "active") ?? designers[0];
	const names = {
		designer: iris?.displayName ?? designer,
		reviewer: reviewer ?? "",
		developer: assignee ?? "",
		pm,
	};
	// Where the Designer's own task stands, step by step (the "How Iris works" panel).
	const reached =
		contract.status === "accepted"
			? 5
			: contract.status === "verifying"
				? 4
				: design?.state === "approved"
					? 3
					: design?.state === "proposed"
						? 2
						: 0;
	const steps = [
		"designerStepLook",
		"designerStepPlan",
		"designerStepApprove",
		"designerStepChange",
		"designerStepReview",
	] as const;

	const told = (e: Event) => {
		const actor =
			ACTORS.map((field) => e.body[field]).find(Boolean) ??
			(e.kind.startsWith("human.") ? "human" : e.agentId);
		if (e.kind === "task.transitioned" && e.body.to)
			return movedWords(nameOf(actor), e.body.to as Contract["status"]);
		const key =
			e.kind === "human.accepted"
				? e.body.subject === "contract"
					? "toldApproved"
					: "toldAccepted"
				: (TOLD[e.kind] ?? "toldOther");
		return t(key, { designer: nameOf(proposal?.agentId) })
			.replace("{who}", nameOf(actor))
			.replace(
				"{status}",
				e.body.to ? statusWord(e.body.to as Contract["status"]) : "",
			)
			.replace("{usd}", dollars(e.body.costUsd ?? 0));
	};

	const onKey = (e: KeyboardEvent) => {
		const at = TABS.indexOf(tab);
		const to = {
			ArrowRight: at + 1,
			ArrowLeft: at - 1,
			Home: 0,
			End: TABS.length - 1,
		}[e.key];
		if (to === undefined) return;
		e.preventDefault();
		const next = TABS[(to + TABS.length) % TABS.length] ?? tab;
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
				{review && (
					<DesignReview
						taskId={id}
						review={review}
						reviews={detail.designReviews ?? []}
						agents={agents}
						events={events}
						designer={names.designer}
						reviewer={names.reviewer}
						builder={names.developer}
					/>
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
				{design && (
					<>
						<p className={styles.muted}>{t("designLead", { designer, pm })}</p>
						<article className={own.letter}>
							{design.state === "proposed" ? (
								<>
									<p>
										<strong>{t("designWaiting", { pm })}</strong>
									</p>
									<p>{t("designWaitingNote", { pm })}</p>
								</>
							) : (
								<>
									<p>
										<strong>
											{t(
												design.state === "approved"
													? "designApproved"
													: "designReturned",
												{ pm, ...when(decision) },
											)}
										</strong>
									</p>
									<p>
										{t(
											design.state === "approved"
												? "designBeingBuiltNote"
												: "designSentBackNote",
											{ designer },
										)}
									</p>
									{design.reason && <p>{`“${design.reason}”`}</p>}
									{design.state === "returned" && (
										<p>
											{t(
												limit <= NTH.length
													? "designReturns"
													: "designReturnsMany",
												{ n: returns, of: limit, nth: NTH[limit - 1] ?? "" },
											)}
										</p>
									)}
								</>
							)}
						</article>
						<article className={own.letter}>
							<p className={styles.muted}>
								{t("designWrote", { designer, ...when(proposal) })}
							</p>
							{design.plan.split(/\n\s*\n/).map((part, at) => (
								// Two paragraphs of a plan may say the same words.
								// biome-ignore lint/suspicious/noArrayIndexKey: the plan's paragraphs never reorder
								<p key={at}>{part}</p>
							))}
						</article>
					</>
				)}
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
					<dd>
						{t(
							contract.risk === "low"
								? "riskLowWhy"
								: contract.risk === "high"
									? "riskHighWhy"
									: "riskMediumWhy",
						)}
					</dd>
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
				{NOTES.some(([kind]) => noteOf(kind)) ? (
					NOTES.map(([kind, key]) => {
						const text = noteOf(kind);
						const by = agentOf(latestNote(events, kind)?.body.writtenBy);
						if (!text) return null;
						return (
							<div key={kind} className={own.letter}>
								{by && <p className={styles.muted}>{signed(key, by)}</p>}
								{text.split(/\n\s*\n/).map((part) => (
									<p key={part}>{part}</p>
								))}
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
						{designWord || statusWord(contract.status)}
					</StatusWord>
					<p className={styles.muted}>{lead}</p>
					{checking && <p>{t("designCheckingNote", names)}</p>}
					{onDesigner && (
						<>
							<p>{t("designOnDesignerNote", names)}</p>
							{iris && (
								<p>
									<Link to={`/team/${iris.id}`}>
										{t("designResume", names)}
									</Link>
								</p>
							)}
						</>
					)}
					{noBrowser && (
						<>
							<p>{t("designNoBrowserNote", names)}</p>
							{iris && (
								<p>
									<Link to={`/team/${iris.id}`}>
										{t("designTurnOnBrowser", names)}
									</Link>
								</p>
							)}
						</>
					)}
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
				{contract.assigneeRole === "ui_ux_designer" && (
					<section className={styles.section} aria-labelledby="designer-works">
						<h2 id="designer-works">{t("designerWorks", { designer })}</h2>
						<ol className={page.steps}>
							{steps.map((key, at) => (
								<li key={key}>
									{t(key, { ...names, designer })}
									{at <= reached && (
										<>
											{" "}
											<strong>
												{t(at < reached ? "stepDone" : "stepNow")}
											</strong>
										</>
									)}
								</li>
							))}
						</ol>
						<dl className={own.facts}>
							<dt>{t("plansSentBack")}</dt>
							<dd>{t("plansOf", { n: returns, of: limit })}</dd>
						</dl>
					</section>
				)}
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
						<p>
							{t(
								contract.status !== "accepted"
									? "taskAddNotYet"
									: team.team.policy.integration === "auto_merge"
										? "taskAddOnItsOwn"
										: team.team.policy.integration === "pull_request"
											? "taskAddByPullRequest"
											: "taskAddByHand",
							)}
						</p>
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

const capital = (word: string) => word.charAt(0).toUpperCase() + word.slice(1);
