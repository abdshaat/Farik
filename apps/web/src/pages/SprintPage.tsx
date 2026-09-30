import { Button, StatusWord } from "@farik/ui";
import { useState } from "react";
import { Link, useParams } from "react-router";
import { useQuery } from "../app/store.ts";
import { statusWord, type TaskStatus } from "../app/words.ts";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import board from "./Board.module.css";
import { plannerOf } from "./Board.tsx";
import { EndSprint } from "./dialogs/EndSprint.tsx";
import { dollars } from "./Gate.tsx";
import { day } from "./PlanPage.tsx";
import styles from "./pages.module.css";
import type { Team } from "./setup/TeamSetup.tsx";

type Sprint = {
	sprintId: string;
	status: "open" | "ended";
	startedAt: string;
	startedBy?: string;
	endedAt?: string | null;
	budgetUsd?: number | null;
	spentUsd: number;
	plannedBy?: string | null;
	taskCount: number;
	doneCount: number;
	tasks: { taskId: string; title: string; status: TaskStatus }[];
	meetings: { thread: string; firstSeq: number; at: string; posts: number }[];
};

export const MEETINGS: Record<string, keyof typeof en | undefined> = {
	planning: "meetingPlanning",
	standup: "meetingStandup",
	review: "meetingReview",
	retro: "meetingRetro",
};

/** A ceremony's block in the channel: its thread and UTC day, which "Read it" opens. */
export const threadAnchor = (thread: string, at: string) =>
	`thread-${thread}-${at.slice(0, 10)}`;

/** One sprint: who started and planned it, its tasks, its meetings, and its spending. */
export function SprintPage() {
	const { id = "" } = useParams();
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { data: sprint } = useQuery<Sprint>("sprint.get", { sprintId: id });
	const [ending, setEnding] = useState(false);
	if (!team || !sprint) return null;
	const agents = team.team.agents;
	const name = (who?: string | null) =>
		who === "human"
			? t("sprintYou")
			: (agents.find((a) => a.id === who)?.displayName ?? who ?? "");
	const n = sprint.sprintId.slice(1);
	const spent = dollars(sprint.spentUsd);

	return (
		<div className={styles.page}>
			<Link to="/board">{t("sprintBack")}</Link>
			<div className={board.head}>
				<h1 className={board.title}>{t("sprintTitle").replace("{n}", n)}</h1>
				<p>
					{[
						t("sprintStartedBy")
							.replace("{day}", day(sprint.startedAt))
							.replace("{who}", name(sprint.startedBy)),
						sprint.plannedBy &&
							t("sprintPlannedBy").replace("{name}", name(sprint.plannedBy)),
						sprint.status === "ended" &&
							t("sprintEndedOn").replace("{day}", day(sprint.endedAt ?? "")),
					]
						.filter(Boolean)
						.join(" ")}
				</p>
				{sprint.status === "open" && (
					<Button onClick={() => setEnding(true)}>{t("sprintEndEarly")}</Button>
				)}
			</div>
			<section className={styles.section} aria-labelledby="sprint-tasks">
				<h2 id="sprint-tasks">{t("sprintTasks")}</h2>
				<p className={styles.muted}>
					{t("sprintTasksDone")
						.replace("{done}", String(sprint.doneCount))
						.replace("{total}", String(sprint.taskCount))}
				</p>
				<ul className={board.rows} aria-labelledby="sprint-tasks">
					{sprint.tasks.map((task) => (
						<li key={task.taskId} className={board.row}>
							<div className={board.rowText}>
								<Link to={`/tasks/${task.taskId}`}>{task.title}</Link>
								<span className={board.small}>{task.taskId}</span>
								<StatusWord
									tone={
										task.status === "accepted" || task.status === "cancelled"
											? "done"
											: "working"
									}
								>
									{statusWord(task.status)}
								</StatusWord>
							</div>
						</li>
					))}
				</ul>
			</section>
			<section className={styles.section} aria-labelledby="sprint-meetings">
				<h2 id="sprint-meetings">{t("sprintMeetings")}</h2>
				<p className={styles.muted}>
					{t("sprintMeetingsLead").replace("{name}", plannerOf(agents))}
				</p>
				{sprint.meetings.length === 0 ? (
					<p>{t("sprintNoMeetings")}</p>
				) : (
					<ul className={board.rows} aria-labelledby="sprint-meetings">
						{sprint.meetings.map((m) => {
							const word = MEETINGS[m.thread];
							return (
								<li key={m.firstSeq} className={board.row}>
									<div className={board.rowText}>
										<span id={`meeting-${m.firstSeq}`}>
											{word ? t(word) : m.thread}
										</span>
										<span className={board.small}>
											{t(m.posts === 1 ? "meetingPostsOne" : "meetingPosts")
												.replace("{posts}", String(m.posts))
												.replace("{day}", day(m.at))}
										</span>
									</div>
									<Link
										className={board.rowLink}
										to={`/channel#${threadAnchor(m.thread, m.at)}`}
										aria-describedby={`meeting-${m.firstSeq}`}
									>
										{t("sprintReadIt")}
									</Link>
								</li>
							);
						})}
					</ul>
				)}
			</section>
			<section className={styles.section} aria-labelledby="sprint-spending">
				<h2 id="sprint-spending">{t("sprintSpending")}</h2>
				<p>
					{sprint.budgetUsd
						? t("sprintSpentOf")
								.replace("{spent}", spent)
								.replace("{budget}", dollars(sprint.budgetUsd))
						: t("sprintSpentNoLimit").replace("{spent}", spent)}
				</p>
				<p className={styles.muted}>{t("costsSprintWhy")}</p>
				<Link to="/costs">{t("sprintSeeCosts")}</Link>
			</section>
			{ending && (
				<EndSprint
					n={n}
					unfinished={sprint.taskCount - sprint.doneCount}
					planner={plannerOf(agents)}
					onClose={() => setEnding(false)}
				/>
			)}
		</div>
	);
}
