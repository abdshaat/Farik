import {
	Avatar,
	type AvatarKey,
	Button,
	KanbanColumn,
	StatusWord,
} from "@catervas/ui";
import { useState } from "react";
import { Link } from "react-router";
import {
	LANES,
	type Lane,
	laneOf,
	type TaskRow,
	taskNumber,
} from "../app/lanes.ts";
import { useQuery } from "../app/store.ts";
import { statusWord } from "../app/words.ts";
import { useWide } from "../shell/Shell.tsx";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import { boardLine, useAllowances } from "./allowances.tsx";
import styles from "./Board.module.css";
import type { Team } from "./setup/TeamSetup.tsx";

type Activity = {
	agentId: string;
	state: string;
	taskId?: string;
	purpose?: string;
};
type Waiting = { taskId: string; kind: string };
type Mark = { word: string; tone: "done" | "working" | "waiting" };

const LANE_WORDS: Record<Lane, keyof typeof en> = {
	planning: "statusPlanning",
	todo: "statusToDo",
	in_progress: "statusInProgress",
	stuck: "statusStuck",
	review: "statusReview",
	done: "statusDone",
};

/** What an agent's session is doing on a task, in the board's words. */
const DOING: Record<string, keyof typeof en> = {
	refine: "markWriting",
	implement: "markBuilding",
	verify: "markReviewing",
	explore: "markPlanning",
};

/** How many of Done's cards that wait on nobody show before "Show all". */
const DONE_SHOWN = 5;

type RiskPick = "any" | TaskRow["risk"];

/** The team's work in lanes, with filters (step 09's plan). */
export function Board() {
	const wide = useWide();
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { data: list } = useQuery<{ tasks: TaskRow[] }>("tasks.list", {});
	const { data: waiting } = useQuery<{ waiting: Waiting[] }>(
		"waiting.list",
		{},
	);
	const { data: activity } = useQuery<{ activity: Activity[] }>(
		"team.activity",
		{},
	);
	// What agents made on other services, never waited for: the board draws without it.
	const allowances = useAllowances();
	const [who, setWho] = useState<string>();
	const [mine, setMine] = useState(false);
	const [epic, setEpic] = useState<string>();
	const [risk, setRisk] = useState<RiskPick>("any");
	const [cancelled, setCancelled] = useState(false);
	const [lane, setLane] = useState<Lane>("planning");
	// Done is opened for the filters it was opened under; another filter closes it again.
	const [doneOpenFor, setDoneOpenFor] = useState<string>();
	const filterKey = [who, mine, epic, risk, cancelled].join("|");
	const doneOpen = doneOpenFor === filterKey;
	if (!team || !list || !waiting || !activity) return null;
	const agents = team.team.agents.filter((a) => a.status !== "retired");
	const tasks = list.tasks;
	const byId = new Map(tasks.map((task) => [task.taskId, task]));
	const waitsOn = new Map(waiting.waiting.map((w) => [w.taskId, w.kind]));
	const epics = tasks.filter(
		(task) =>
			task.kind === "epic" &&
			task.status !== "accepted" &&
			task.status !== "cancelled",
	);

	const markOf = (task: TaskRow): Mark | undefined => {
		const kind = waitsOn.get(task.taskId);
		if (kind === "help") return { word: t("statusHelp"), tone: "waiting" };
		if (kind) return { word: t("statusWaiting"), tone: "waiting" };
		const doing = activity.activity.find(
			(a) => a.state === "working" && a.taskId === task.taskId,
		);
		// The Designer's review session checks the screens (step 12's design review).
		const designer =
			doing?.purpose === "verify" &&
			agents.find((a) => a.id === doing.agentId)?.role === "ui_ux_designer";
		if (designer) return { word: t("designChecking"), tone: "working" };
		const purpose = doing?.purpose && DOING[doing.purpose];
		if (purpose) return { word: t(purpose), tone: "working" };
		if (task.status === "accepted")
			return { word: t("markAccepted"), tone: "done" };
		if (task.status === "cancelled")
			return { word: t("statusCancelled"), tone: "done" };
		if (task.status === "rejected")
			return { word: statusWord("rejected"), tone: "working" };
		if (task.status === "escalated")
			return {
				word: statusWord(
					"escalated",
					task.awaitingApproval ? "approval" : undefined,
				),
				tone: "waiting",
			};
		return undefined;
	};

	const shown = tasks.filter(
		(task) =>
			(cancelled || task.status !== "cancelled") &&
			(!who || task.assigneeId === who) &&
			(!mine || waitsOn.has(task.taskId)) &&
			(!epic || task.taskId === epic || task.parent === epic) &&
			(risk === "any" || task.risk === risk),
	);
	const inLane = (l: Lane) => shown.filter((task) => laneOf(task) === l);
	const lanes = LANES;
	const partsOf = (task: TaskRow) =>
		tasks.filter((p) => p.parent === task.taskId);

	const row = (task: TaskRow) => {
		const who = agents.find((a) => a.id === task.assigneeId);
		const mark = markOf(task);
		const parts = partsOf(task);
		const parent = task.parent ? byId.get(task.parent) : undefined;
		const meta = parent ? `${task.taskId} · ${parent.title}` : task.taskId;
		return (
			<li key={task.taskId} className={styles.row}>
				{who?.avatar && (
					<Avatar
						avatarKey={who.avatar as AvatarKey}
						name={who.displayName}
						size={32}
					/>
				)}
				<div className={styles.rowText}>
					<Link
						to={`/tasks/${task.taskId}`}
						className={styles.title3}
						title={task.title}
					>
						{task.title}
					</Link>
					<span className={styles.meta} title={meta}>
						{meta}
					</span>
					{task.kind === "epic" && (
						<span className={styles.small}>
							{t(parts.length === 1 ? "epicPartsOne" : "epicParts")
								.replace("{parts}", String(parts.length))
								.replace(
									"{done}",
									String(parts.filter((p) => p.status === "accepted").length),
								)}
						</span>
					)}
					{mark ? (
						<StatusWord tone={mark.tone}>{mark.word}</StatusWord>
					) : task.designReviewState === "waiting_on_designer" ? (
						<StatusWord tone="waiting">{t("designOnDesigner")}</StatusWord>
					) : task.designReviewState === "designer_needs_browser" ? (
						<StatusWord tone="waiting">{t("designNoBrowser")}</StatusWord>
					) : null}
				</div>
			</li>
		);
	};

	const column = (l: Lane) => {
		const rows = inLane(l);
		// Done shows what waits on the owner and the most recent others.
		const recent = new Set(
			l === "done"
				? rows
						.filter((task) => !waitsOn.has(task.taskId))
						.sort((a, b) => taskNumber(b.taskId) - taskNumber(a.taskId))
						.slice(0, DONE_SHOWN)
						.map((task) => task.taskId)
				: [],
		);
		const kept = rows.filter(
			(task) =>
				l !== "done" || waitsOn.has(task.taskId) || recent.has(task.taskId),
		);
		const folds = kept.length < rows.length;
		const visible = doneOpen ? rows : kept;
		return (
			<KanbanColumn
				key={l}
				id={`lane-${l}`}
				title={t(LANE_WORDS[l])}
				count={rows.length}
			>
				{rows.length === 0 ? (
					<p className={styles.small}>{t("laneEmpty")}</p>
				) : (
					<ul
						className={styles.rows}
						{...(l === "done" ? { id: "lane-done-rows" } : {})}
					>
						{visible.map(row)}
					</ul>
				)}
				{folds && (
					<Button
						kind="quiet"
						expanded={doneOpen}
						controls="lane-done-rows"
						onClick={() => setDoneOpenFor(doneOpen ? undefined : filterKey)}
					>
						{doneOpen
							? t("boardShowFewer")
							: t("boardShowAll", { count: rows.length })}
					</Button>
				)}
			</KanbanColumn>
		);
	};

	const pick = (on: boolean, word: string, click: () => void, key?: string) => (
		<button
			key={key ?? word}
			type="button"
			className={styles.chip}
			aria-pressed={on}
			onClick={click}
		>
			{word}
		</button>
	);

	return (
		<div className={styles.page}>
			<div className={styles.head}>
				<h1 className={styles.title}>{t("board")}</h1>
				{allowances && allowances.rows.length > 0 && (
					<section aria-label={t("allowBoardHeading")}>
						<ul className={styles.allowances}>
							{allowances.rows.map((row) => (
								<li key={`${row.agent}-${row.server}-${row.tool}`}>
									{boardLine(
										row,
										agents.find((a) => a.id === row.agent)?.displayName ??
											row.agent,
									)}
								</li>
							))}
						</ul>
					</section>
				)}
			</div>
			<fieldset className={styles.filters}>
				<legend className={styles.hidden}>{t("boardFilters")}</legend>
				{pick(!who, t("filterEveryone"), () => setWho(undefined))}
				{agents.map((a) =>
					pick(who === a.id, a.displayName, () => setWho(a.id), a.id),
				)}
				{pick(mine, t("filterWaiting"), () => setMine(!mine))}
				{epics.map((e) =>
					pick(
						epic === e.taskId,
						e.title,
						() => setEpic(epic === e.taskId ? undefined : e.taskId),
						e.taskId,
					),
				)}
				<details className={styles.more}>
					<summary>{t("filterMore")}</summary>
					<label>
						{t("filterRisk")}{" "}
						<select
							value={risk}
							onChange={(e) => setRisk(e.target.value as RiskPick)}
						>
							<option value="any">{t("filterRiskAny")}</option>
							<option value="low">{t("riskLow")}</option>
							<option value="medium">{t("riskMedium")}</option>
							<option value="high">{t("riskHigh")}</option>
						</select>
					</label>
					<label>
						<input
							type="checkbox"
							checked={cancelled}
							onChange={() => setCancelled(!cancelled)}
						/>{" "}
						{t("filterCancelled")}
					</label>
				</details>
			</fieldset>
			{/* The lanes' own headings sit under this one. */}
			<h2 className={styles.hidden}>{t("laneTabs")}</h2>
			{wide ? (
				<div className={styles.lanes}>{lanes.map(column)}</div>
			) : (
				<>
					<fieldset className={styles.tabs}>
						<legend className={styles.hidden}>{t("laneTabs")}</legend>
						{lanes.map((l) =>
							pick(
								lane === l,
								`${t(LANE_WORDS[l])} ${inLane(l).length}`,
								() => setLane(l),
								l,
							),
						)}
					</fieldset>
					{column(lane)}
				</>
			)}
		</div>
	);
}
