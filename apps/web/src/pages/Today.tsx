import { Avatar, type AvatarKey, Button, RoleTag, uiStrings } from "@farik/ui";
import { type FormEvent, useState } from "react";
import { Link, useNavigate } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import { codeOf, sentence } from "../app/words.ts";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import type { Agent, Team } from "./setup/TeamSetup.tsx";
import styles from "./Today.module.css";

type Activity = { agentId: string; state: string; line: string };
type Kind = "approval" | "acceptance" | "question" | "help" | "integration";
type Waiting = {
	taskId: string;
	kind: Kind;
	agentId: string | null;
	title: string;
	line: string;
};
type Moved = { at: string; line: string };
type Sprint = { sprintId: string; done: number; total: number } | null;
type Check = { passed: boolean };

/** Each kind's title, button word and page (the step 08 plan's routes). */
const KINDS: Record<
	Kind,
	{ title: keyof typeof en; word: keyof typeof en; page: string }
> = {
	approval: { title: "waitingApproval", word: "waitingReview", page: "plan" },
	acceptance: {
		title: "waitingAcceptance",
		word: "waitingReview",
		page: "accept",
	},
	question: {
		title: "waitingQuestion",
		word: "waitingAnswer",
		page: "questions",
	},
	help: { title: "waitingHelp", word: "waitingHelpButton", page: "help" },
	integration: {
		title: "waitingIntegration",
		word: "waitingAdd",
		page: "accept",
	},
};

const DAY_MS = 86_400_000;

/** The home page: the team, the request box, what waits on the human, and what moved. */
export function Today() {
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { data: activity } = useQuery<{ activity: Activity[] }>(
		"team.activity",
		{},
	);
	const { data: waiting } = useQuery<{ waiting: Waiting[] }>(
		"waiting.list",
		{},
	);
	// Once, so the query's key stays the same between renders.
	const [since] = useState(() => new Date(Date.now() - DAY_MS).toISOString());
	const { data: moved } = useQuery<{ moved: Moved[] }>("moved.since", {
		since,
	});
	const { data: sprint } = useQuery<Sprint>("sprint.current", {});
	const agents = team?.team.agents ?? [];
	const agent = (id: string | null) => agents.find((a) => a.id === id);
	const pm = agents.find(
		(a) => a.role === "product_manager" && a.status !== "retired",
	);
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("today")}</h1>
			<div className={styles.band}>
				{sprint && (
					<p>
						{t("sprintLine", {
							n: sprint.sprintId.replace(/^S/, ""),
							done: String(sprint.done),
							total: String(sprint.total),
						})}
					</p>
				)}
				<ul className={styles.agents} aria-label={t("teamBand")}>
					{activity?.activity.map((one) => (
						<AgentEntry
							key={one.agentId}
							one={one}
							agent={agent(one.agentId)}
						/>
					))}
				</ul>
			</div>
			<RequestBox
				pmName={pm?.displayName ?? uiStrings.roleName.product_manager}
			/>
			{waiting && (
				<section className={styles.section} aria-labelledby="waiting-heading">
					<h2 id="waiting-heading">
						{t("waitingTitle", { count: String(waiting.waiting.length) })}
					</h2>
					{waiting.waiting.length === 0 ? (
						<p className={styles.muted}>{t("waitingNone")}</p>
					) : (
						<ul className={styles.rows} aria-label={t("waitingList")}>
							{waiting.waiting.map((item) => (
								<WaitingRow
									key={`${item.kind}-${item.taskId}`}
									item={item}
									agent={agent(item.agentId)}
								/>
							))}
						</ul>
					)}
				</section>
			)}
			{moved && (
				<section className={styles.section} aria-labelledby="moved-heading">
					<h2 id="moved-heading">{t("movedTitle")}</h2>
					{moved.moved.length === 0 ? (
						<p className={styles.muted}>{t("movedNone")}</p>
					) : (
						<ul className={styles.moved} aria-label={t("movedTitle")}>
							{moved.moved.map((one) => (
								<li key={`${one.at}-${one.line}`}>
									{/* Times are HH:MM UTC, as the team's own lines say them. */}
									<time dateTime={one.at}>{one.at.slice(11, 16)}</time>
									<span>{one.line}</span>
								</li>
							))}
						</ul>
					)}
				</section>
			)}
		</div>
	);
}

function AgentEntry({
	one,
	agent,
}: {
	one: Activity;
	agent: Agent | undefined;
}) {
	const name = agent?.displayName ?? one.agentId;
	return (
		<li className={styles.agent}>
			{agent?.avatar && (
				<Avatar avatarKey={agent.avatar as AvatarKey} name={name} size={48} />
			)}
			<span className={styles.who}>
				<strong>{name}</strong> {agent && <RoleTag role={agent.role} />}
			</span>
			<span className={styles.line}>{one.line}</span>
		</li>
	);
}

/** The request box: the person's words, sent as `request.file`, and the request's page after. */
function RequestBox({ pmName }: { pmName: string }) {
	const { client } = useConnection();
	const navigate = useNavigate();
	const [text, setText] = useState("");
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const send = async (e: FormEvent) => {
		e.preventDefault();
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const filed = (await client.call("request.file", { text })) as {
				taskId: string;
			};
			navigate(`/requests/${filed.taskId}`);
		} catch (error) {
			// The length refusal is known by its code, and said in the page's own sentence.
			setRefusal(
				codeOf(error) === "too_short"
					? t("requestTooShort")
					: sentence((error as Error).message),
			);
			setBusy(false);
		}
	};
	return (
		<form className={styles.request} onSubmit={send}>
			<label htmlFor="request" className={styles.label}>
				{t("requestLabel")}
			</label>
			<div className={styles.prompt}>
				<span aria-hidden="true">&gt;</span>
				<textarea
					id="request"
					rows={3}
					value={text}
					aria-describedby="request-hint"
					onChange={(e) => setText(e.target.value)}
				/>
				{/* The page's only idle motion; stopped under reduced motion. */}
				{text === "" && <span className={styles.cursor} aria-hidden="true" />}
			</div>
			<p id="request-hint" className={styles.muted}>
				{t("requestHint", { name: pmName })}
			</p>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
			<div>
				<Button kind="primary" type="submit" busy={busy}>
					{t("requestSend")}
				</Button>
			</div>
		</form>
	);
}

function WaitingRow({
	item,
	agent,
}: {
	item: Waiting;
	agent: Agent | undefined;
}) {
	const kind = KINDS[item.kind];
	const name = agent?.displayName ?? item.agentId ?? "";
	const titleId = `waiting-${item.kind}-${item.taskId}`;
	return (
		<li className={styles.row}>
			{agent?.avatar && (
				<Avatar avatarKey={agent.avatar as AvatarKey} name={name} size={32} />
			)}
			<div className={styles.rowText}>
				<strong id={titleId}>
					{t(kind.title, { title: item.title, agent: name })}
				</strong>
				<span>{item.line}</span>
				{item.kind === "acceptance" && <ChecksPassed taskId={item.taskId} />}
			</div>
			<Link
				className={styles.action}
				to={`/tasks/${item.taskId}/${kind.page}`}
				aria-describedby={titleId}
			>
				{t(kind.word)}
			</Link>
		</li>
	);
}

/** "All N of Farik's checks passed.", shown only when every check of the task passed. */
function ChecksPassed({ taskId }: { taskId: string }) {
	const { data } = useQuery<{ checks: Check[] }>("task.checks", {
		task_id: taskId,
	});
	const checks = data?.checks ?? [];
	if (checks.length === 0 || !checks.every((c) => c.passed)) return null;
	return <span>{t("checksPassed", { count: String(checks.length) })}</span>;
}
