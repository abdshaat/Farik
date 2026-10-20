import { Button, Choice, Dialog, TextField } from "@catervas/ui";
import { useId, useState } from "react";
import { Link } from "react-router";
import { useCommand } from "../app/command.ts";
import { Bullets } from "../components/Bullets.tsx";
import { t } from "../strings/t.ts";
import styles from "./GoalCard.module.css";
import type { Team } from "./setup/TeamSetup.tsx";

/** One reading of the goal's number: from a service an agent reads, or told by the owner. */
export type GoalReading = {
	value: number;
	source: "connector" | "owner";
	server: string | null;
	agent: string | null;
	at: string;
};

/** The goal that stands, as `goal.get` answers it (SPEC 5.18). */
export type Goal = {
	goal: string;
	words: string;
	target: number | null;
	unit: string | null;
	server: string | null;
	reader: string | null;
	planner: string | null;
	status: "active" | "paused" | "met" | "stalled";
	stall: "nothing_planned" | "no_progress" | null;
	/** Newest first. */
	readings: GoalReading[];
	open: number;
	done: number;
	waitingOnOwner: number;
	planning: boolean;
};

/** What `goal.get` answers: the goal that stands and the last one ended. */
export type GoalState = {
	goal: Goal | null;
	lastEnded: {
		goal: string;
		words: string;
		outcome: "reached" | "dropped";
		at: string;
	} | null;
};

const MULTIPLES: Record<string, number> = { k: 1_000, m: 1_000_000 };
const NUMBER = /(\d{1,3}(?:,\d{3})+|\d+)(\.\d+)?([kKmM])?/;
/** Where a unit ends: a comma, a full stop, or one of these words. */
const UNIT_END = /[,.]|\s(?:and|by|in|within)\s/i;

/**
 * The number and unit in the owner's words: the first number ("10", "5,000", "1.5k", "2M"), and
 * the words after it up to a comma, a full stop, "and", "by", "in" or "within", cut to 40
 * characters. Nothing when there is no number or no unit after it.
 */
export function guessTarget(
	words: string,
): { target: number; unit: string } | null {
	const found = NUMBER.exec(words);
	if (!found) return null;
	const digits = Number(`${found[1]?.replaceAll(",", "")}${found[2] ?? ""}`);
	const target = Number(
		(digits * (MULTIPLES[(found[3] ?? "").toLowerCase()] ?? 1)).toPrecision(12),
	);
	const after = words.slice(found.index + found[0].length);
	const end = UNIT_END.exec(after);
	const unit = (end ? after.slice(0, end.index) : after).trim().slice(0, 40);
	return unit === "" ? null : { target, unit: unit.trim() };
}

const number = (value: number) => new Intl.NumberFormat("en").format(value);

/** A service's name as the owner reads it: the connector's name with a capital. */
const serviceName = (server: string) =>
	server.charAt(0).toUpperCase() + server.slice(1);

/** How long ago `at` was, in words. */
function ago(at: string): string {
	const minutes = Math.max(
		0,
		Math.floor((Date.now() - new Date(at).getTime()) / 60_000),
	);
	if (minutes < 1) return t("goalAgoNow");
	if (minutes < 60)
		return minutes === 1
			? t("goalAgoMinute")
			: t("goalAgoMinutes", { count: minutes });
	const hours = Math.floor(minutes / 60);
	if (hours < 48)
		return hours === 1 ? t("goalAgoHour") : t("goalAgoHours", { count: hours });
	const days = Math.floor(hours / 24);
	return t("goalAgoDays", { count: days });
}

type Dialogs = "set" | "edit" | "report" | "reached" | "end";

/**
 * The team's goal on Today: a quiet card with no goal, else the goal, how far the team has come,
 * what it is doing now, and the owner's word on it. `again` asks `goal.get` again.
 */
export function GoalCard({
	state,
	team,
	again,
}: {
	state: GoalState;
	team: Team;
	again: () => void;
}) {
	const [open, setOpen] = useState<Dialogs | null>(null);
	const [more, setMore] = useState(false);
	const menu = useId();
	const done = () => {
		setOpen(null);
		setMore(false);
		again();
	};
	const { busy, refusal, send } = useCommand(done);
	const { goal, lastEnded } = state;
	const nameOf = (id: string | null) =>
		team.agents.find((agent) => agent.id === id)?.displayName ?? id ?? "";
	return (
		<section
			className={styles.card}
			aria-label={goal === null ? t("goalCardTitle") : t("goalCardLabel")}
		>
			{goal === null ? (
				<>
					<div className={styles.quiet}>
						<div>
							<strong>{t("goalCardTitle")}</strong>
							<p>{t("goalCardLead")}</p>
						</div>
						<Button onClick={() => setOpen("set")}>{t("goalSet")}</Button>
					</div>
					{lastEnded && (
						<p className={styles.last}>
							{t(
								lastEnded.outcome === "reached"
									? "goalLastReached"
									: "goalLastDropped",
								{
									date: new Date(lastEnded.at).toLocaleDateString("en-GB", {
										day: "numeric",
										month: "short",
									}),
									words: lastEnded.words,
								},
							)}
						</p>
					)}
				</>
			) : (
				<div className={styles.standing}>
					<h2 className={styles.words}>{goal.words}</h2>
					<Progress goal={goal} nameOf={nameOf} />
					<Doing goal={goal} team={team} nameOf={nameOf} />
					<div className={styles.buttons}>
						{goal.status === "stalled" && (
							<Button
								kind="primary"
								busy={busy}
								onClick={() =>
									send({
										command: "goal_resume",
										body: { goal: goal.goal },
									})
								}
							>
								{t("goalKeepGoing")}
							</Button>
						)}
						<Button
							kind={goal.status === "met" ? "primary" : "secondary"}
							onClick={() => setOpen("reached")}
						>
							{t("goalMarkReached")}
						</Button>
						{goal.target !== null && (
							<Button onClick={() => setOpen("report")}>
								{t("goalUpdate")}
							</Button>
						)}
						<Button
							kind="quiet"
							expanded={more}
							controls={menu}
							onClick={() => setMore(!more)}
						>
							{t("goalMore")}
						</Button>
					</div>
					{more && (
						<div id={menu} className={styles.more}>
							<Button kind="quiet" onClick={() => setOpen("edit")}>
								{t("goalEdit")}
							</Button>
							<Button
								kind="quiet"
								busy={busy}
								onClick={() =>
									send({
										command:
											goal.status === "paused" ? "goal_resume" : "goal_pause",
										body: { goal: goal.goal },
									})
								}
							>
								{goal.status === "paused" ? t("goalResume") : t("goalPause")}
							</Button>
							<Button kind="quiet" onClick={() => setOpen("end")}>
								{t("goalEnd")}
							</Button>
						</div>
					)}
					{refusal && (
						<p role="alert" className={styles.alert}>
							{refusal}
						</p>
					)}
				</div>
			)}
			{(open === "set" || open === "edit") && (
				<GoalForm
					goal={open === "edit" ? goal : null}
					team={team}
					onClose={() => setOpen(null)}
					onDone={done}
				/>
			)}
			{open === "report" && goal && (
				<ReportDialog goal={goal} onClose={() => setOpen(null)} onDone={done} />
			)}
			{(open === "reached" || open === "end") && goal && (
				<ConfirmDialog
					goal={goal}
					reached={open === "reached"}
					onClose={() => setOpen(null)}
					onDone={done}
				/>
			)}
		</section>
	);
}

/** The number against its target, and where it was read. */
function Progress({
	goal,
	nameOf,
}: {
	goal: Goal;
	nameOf: (id: string | null) => string;
}) {
	if (goal.target === null) {
		return <p className={styles.note}>{t("goalWhenYouSay")}</p>;
	}
	const latest = goal.readings[0];
	const unit = goal.unit ?? "";
	return (
		<div className={styles.card}>
			{latest ? (
				<>
					<p className={styles.count}>
						{t("goalProgress", {
							value: number(latest.value),
							target: number(goal.target),
							unit,
						})}
					</p>
					<div className={styles.meter} aria-hidden="true">
						<span
							style={{
								width: `${Math.min(100, (latest.value / goal.target) * 100)}%`,
							}}
						/>
					</div>
					<p className={styles.note}>
						{latest.source === "owner"
							? t("goalFromYou", { ago: ago(latest.at) })
							: t("goalFromService", {
									service: serviceName(latest.server ?? goal.server ?? ""),
									name: nameOf(latest.agent),
									ago: ago(latest.at),
								})}
					</p>
				</>
			) : (
				<p className={styles.note}>{t("goalNotCounted")}</p>
			)}
			{goal.server !== null && goal.reader === null && (
				<p className={styles.note}>
					{t("goalNobodyHas", { service: serviceName(goal.server) })}
				</p>
			)}
		</div>
	);
}

/** What the team is doing about the goal, or what it asks of the owner. */
function Doing({
	goal,
	team,
	nameOf,
}: {
	goal: Goal;
	team: Team;
	nameOf: (id: string | null) => string;
}) {
	const planner = nameOf(goal.planner);
	if (goal.status === "met" && goal.target !== null) {
		const latest = goal.readings[0];
		return (
			<p className={styles.asking}>
				{t("goalMet", {
					value: number(latest?.value ?? goal.target),
					target: number(goal.target),
					unit: goal.unit ?? "",
				})}
			</p>
		);
	}
	if (goal.status === "stalled") {
		return (
			<p className={styles.asking}>
				{goal.stall === "nothing_planned"
					? t("goalStallNothing", { name: planner })
					: goal.target === null
						? t("goalStallCheckIn")
						: t("goalStallNoProgress")}
			</p>
		);
	}
	let line: string;
	if (goal.status === "paused") line = t("goalPaused");
	else if (goal.planning) line = t("goalPlanning", { name: planner });
	else if (goal.open > 0)
		line =
			goal.open === 1
				? t("goalUnderWayOne", { done: goal.done })
				: t("goalUnderWay", { open: goal.open, done: goal.done });
	else {
		const off = goal.planner
			? undefined
			: team.agents.find(
					(agent) =>
						agent.role === "product_manager" && agent.status !== "retired",
				);
		line = off
			? t("goalPlannerOff", { name: off.displayName, status: off.status })
			: t("goalNextRound");
	}
	return <p className={styles.status}>{line}</p>;
}

/** The services a goal's number can be read from: one for each connector an active agent has. */
function services(team: Team): { server: string; name: string }[] {
	const found = new Map<string, string>();
	for (const agent of team.agents) {
		if (agent.status !== "active") continue;
		for (const server of agent.mcpServers ?? []) {
			if (server.source !== "builtin" && !found.has(server.name))
				found.set(server.name, agent.displayName);
		}
	}
	return [...found].map(([server, name]) => ({ server, name }));
}

/** Setting a goal, or, with `goal`, changing it. */
function GoalForm({
	goal,
	team,
	onClose,
	onDone,
}: {
	goal: Goal | null;
	team: Team;
	onClose: () => void;
	onDone: () => void;
}) {
	const [words, setWords] = useState(goal?.words ?? "");
	const [target, setTarget] = useState(
		goal?.target == null ? "" : String(goal.target),
	);
	const [unit, setUnit] = useState(goal?.unit ?? "");
	const [how, setHow] = useState(goal?.server ?? "owner");
	// The number is guessed from the words until the owner changes it.
	const [touched, setTouched] = useState(goal !== null);
	const [invalid, setInvalid] = useState(false);
	const { busy, refusal, code, send } = useCommand(onDone);
	const counted = target.trim() !== "";
	const change = (text: string) => {
		setWords(text);
		if (touched) return;
		const guess = guessTarget(text);
		setTarget(guess ? String(guess.target) : "");
		setUnit(guess ? guess.unit : "");
	};
	const submit = () => {
		const value = Number(target.replaceAll(",", ""));
		if (counted && !(value > 0)) {
			setInvalid(true);
			return;
		}
		setInvalid(false);
		const body: Record<string, unknown> = { words: words.trim() };
		if (goal) body.goal = goal.goal;
		if (counted) body.target = value;
		if (unit.trim() !== "") body.unit = unit.trim();
		if (counted && how !== "owner") body.server = how;
		send({ command: goal ? "goal_edit" : "goal_set", body });
	};
	const options = services(team);
	return (
		<Dialog
			open
			title={goal ? t("goalEditTitle") : t("goalSet")}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("goalCancel")}</Button>
					<Button
						kind="primary"
						busy={busy}
						disabled={words.trim() === ""}
						onClick={submit}
					>
						{goal ? t("goalSaveEdit") : t("goalSave")}
					</Button>
				</>
			}
		>
			<div className={styles.form}>
				<TextField
					id="goal-words"
					label={t("goalWords")}
					info={<Bullets text={t("goalInfo")} />}
					value={words}
					onChange={change}
				/>
				<div className={styles.pair}>
					<TextField
						id="goal-target"
						label={t("goalTarget")}
						hint={t("goalTargetHint")}
						value={target}
						{...(invalid && { error: t("goalTargetInvalid") })}
						onChange={(value) => {
							setTouched(true);
							setTarget(value);
						}}
					/>
					<TextField
						id="goal-unit"
						label={t("goalUnit")}
						value={unit}
						onChange={(value) => {
							setTouched(true);
							setUnit(value);
						}}
					/>
				</div>
				{counted && (
					<Choice
						name="goal-how"
						legend={t("goalHow")}
						value={how}
						onChange={setHow}
						options={[
							{ value: "owner", label: t("goalHowOwner") },
							...options.map(({ server, name }) => ({
								value: server,
								label: t("goalHowService", {
									service: serviceName(server),
									name,
								}),
							})),
						]}
					/>
				)}
				{refusal && (
					<p role="alert" className={styles.alert}>
						{code === "goal_needs_daily_budget" ? (
							<Link to="/settings/spending">{refusal}</Link>
						) : (
							refusal
						)}
					</p>
				)}
			</div>
		</Dialog>
	);
}

/** Telling the team the goal's number. */
function ReportDialog({
	goal,
	onClose,
	onDone,
}: {
	goal: Goal;
	onClose: () => void;
	onDone: () => void;
}) {
	const [value, setValue] = useState("");
	const { busy, refusal, send } = useCommand(onDone);
	const parsed = Number(value.replaceAll(",", ""));
	return (
		<Dialog
			open
			title={t("goalReportTitle")}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("goalCancel")}</Button>
					<Button
						kind="primary"
						busy={busy}
						disabled={value.trim() === "" || Number.isNaN(parsed)}
						onClick={() =>
							send({
								command: "goal_report",
								body: { goal: goal.goal, value: parsed },
							})
						}
					>
						{t("goalReportSave")}
					</Button>
				</>
			}
		>
			<div className={styles.form}>
				<TextField
					id="goal-report"
					label={t("goalReportLabel", { unit: goal.unit ?? "" })}
					value={value}
					onChange={setValue}
				/>
				{refusal && (
					<p role="alert" className={styles.alert}>
						{refusal}
					</p>
				)}
			</div>
		</Dialog>
	);
}

/** Marking the goal reached, or ending it: each asks once. */
function ConfirmDialog({
	goal,
	reached,
	onClose,
	onDone,
}: {
	goal: Goal;
	reached: boolean;
	onClose: () => void;
	onDone: () => void;
}) {
	const { busy, refusal, send } = useCommand(onDone);
	return (
		<Dialog
			open
			title={
				reached
					? t("goalConfirmReached", { words: goal.words })
					: t("goalEndTitle")
			}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("goalNotYet")}</Button>
					<Button
						kind="primary"
						busy={busy}
						onClick={() =>
							send({
								command: "goal_end",
								body: {
									goal: goal.goal,
									outcome: reached ? "reached" : "dropped",
								},
							})
						}
					>
						{reached ? t("goalYesReached") : t("goalYesEnd")}
					</Button>
				</>
			}
		>
			<p>{reached ? t("goalReachedNote") : t("goalEndNote")}</p>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
		</Dialog>
	);
}
