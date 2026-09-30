import {
	Avatar,
	type AvatarKey,
	Button,
	KanbanColumn,
	StatusWord,
} from "@farik/ui";
import { useState } from "react";
import { Link } from "react-router";
import { LANES, type Lane, laneOf, type TaskRow } from "../app/lanes.ts";
import { useQuery } from "../app/store.ts";
import { statusWord } from "../app/words.ts";
import { useWide } from "../shell/Shell.tsx";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import styles from "./Board.module.css";
import { EndSprint } from "./dialogs/EndSprint.tsx";
import { StartSprint } from "./dialogs/StartSprint.tsx";
import type { Agent, Team } from "./setup/TeamSetup.tsx";

type Activity = {
	agentId: string;
	state: string;
	taskId?: string;
	purpose?: string;
};
type Waiting = { taskId: string; kind: string };
type Sprint = { sprintId: string; done: number; total: number } | null;
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

type SprintPick = "all" | "this" | "none";
type RiskPick = "any" | TaskRow["risk"];

/** The team's work in lanes, with filters and the sprint's controls (step 09's plan). */
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
	const { data: sprint } = useQuery<Sprint>("sprint.current", {});
	const { data: sprints } = useQuery<{ sprints: unknown[] }>(
		"sprints.list",
		{},
	);
	const [who, setWho] = useState<string>();
	const [mine, setMine] = useState(false);
	const [epic, setEpic] = useState<string>();
	const [inSprint, setInSprint] = useState<SprintPick>("all");
	const [risk, setRisk] = useState<RiskPick>("any");
	const [cancelled, setCancelled] = useState(false);
	const [lane, setLane] = useState<Lane>("planning");
	const [dialog, setDialog] = useState<"start" | "end">();

	if (
		!team ||
		!list ||
		!waiting ||
		!activity ||
		sprint === undefined ||
		!sprints
	)
		return null;
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
			(inSprint === "all" ||
				(inSprint === "this"
					? !!sprint && task.sprint === sprint.sprintId
					: !task.sprint)) &&
			(risk === "any" || task.risk === risk),
	);
	const inLane = (l: Lane) => shown.filter((task) => laneOf(task) === l);
	const n = (sprint?.sprintId ?? `S${sprints.sprints.length + 1}`).slice(1);

	const row = (task: TaskRow) => {
		const who = agents.find((a) => a.id === task.assigneeId);
		const mark = markOf(task);
		const parts = tasks.filter((p) => p.parent === task.taskId);
		const parent = task.parent ? byId.get(task.parent) : undefined;
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
					{parent && <span className={styles.small}>{parent.title}</span>}
					<Link to={`/tasks/${task.taskId}`}>{task.title}</Link>
					<span className={styles.small}>{task.taskId}</span>
					{task.kind === "epic" && (
						<span className={styles.small}>
							{t("epicParts")
								.replace("{parts}", String(parts.length))
								.replace(
									"{done}",
									String(parts.filter((p) => p.status === "accepted").length),
								)}
						</span>
					)}
					{mark && <StatusWord tone={mark.tone}>{mark.word}</StatusWord>}
				</div>
			</li>
		);
	};

	const column = (l: Lane) => {
		const rows = inLane(l);
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
					<ul className={styles.rows}>{rows.map(row)}</ul>
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
				{sprint ? (
					<p>
						<Link to={`/sprints/${sprint.sprintId}`}>
							{t("sprintLine")
								.replace("{n}", n)
								.replace("{done}", String(sprint.done))
								.replace("{total}", String(sprint.total))}
						</Link>
					</p>
				) : (
					<p>{t("sprintNone")}</p>
				)}
				<Button
					kind={sprint ? "secondary" : "primary"}
					onClick={() => setDialog(sprint ? "end" : "start")}
				>
					{t(sprint ? "sprintEndEarly" : "sprintStart")}
				</Button>
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
						{t("filterSprint")}{" "}
						<select
							value={inSprint}
							onChange={(e) => setInSprint(e.target.value as SprintPick)}
						>
							<option value="all">{t("filterSprintAll")}</option>
							<option value="this">{t("filterSprintThis")}</option>
							<option value="none">{t("filterSprintNone")}</option>
						</select>
					</label>
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
				<div className={styles.lanes}>{LANES.map(column)}</div>
			) : (
				<>
					<fieldset className={styles.tabs}>
						<legend className={styles.hidden}>{t("laneTabs")}</legend>
						{LANES.map((l) =>
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
			{dialog === "start" && (
				<StartSprint
					n={n}
					planner={plannerOf(agents)}
					ready={
						tasks.filter(
							(task) => task.status === "ready" && !task.parent && !task.sprint,
						).length
					}
					onClose={() => setDialog(undefined)}
				/>
			)}
			{dialog === "end" && sprint && (
				<EndSprint
					n={n}
					unfinished={sprint.total - sprint.done}
					planner={plannerOf(agents)}
					onClose={() => setDialog(undefined)}
				/>
			)}
		</div>
	);
}

/** The sprint's planner: the Scrum Master, or the Product Manager when there is none (spec 5.9). */
export function plannerOf(agents: Agent[]): string {
	const of = (role: Agent["role"]) =>
		agents.find((a) => a.role === role)?.displayName;
	return of("scrum_master") ?? of("product_manager") ?? "";
}
