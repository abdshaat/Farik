import { Link } from "../app/links.tsx";
import { AgentPicture } from "../components/AgentPicture.tsx";
import { t } from "../strings/t.ts";
import type { Agent } from "./setup/TeamSetup.tsx";
import styles from "./Today.module.css";

export type FlaggedRow = {
	title: string;
	needs: string;
	at: string;
	taskId?: string;
	goal?: string;
	by?: string;
};

/**
 * What only the owner can give, as the team flagged it while autopilot runs (ADR 0082): the
 * agent's name and the team's own words as text, and a link to the task. A goal's row has none:
 * the goal card is on the page.
 */
export function FlaggedByTheTeam({
	rows,
	agents,
}: {
	rows: FlaggedRow[];
	agents: Agent[];
}) {
	if (rows.length === 0) return null;
	return (
		<section className={styles.section} aria-labelledby="flagged-heading">
			<h2 id="flagged-heading">
				{t("flaggedTitle", { count: String(rows.length) })}
			</h2>
			<ul
				className={styles.rows}
				aria-label={t("flaggedTitle", { count: rows.length })}
			>
				{rows.map((row) => {
					const one = agents.find((a) => a.id === row.by);
					if (row.goal !== undefined)
						return (
							<li key={`goal-${row.goal}-${row.at}`} className={styles.row}>
								<div className={styles.rowText}>
									<strong>{t("flaggedGoal", { needs: row.needs })}</strong>
								</div>
							</li>
						);
					const name =
						row.by === undefined || row.by === "catervas"
							? t("brand")
							: (one?.displayName ?? row.by);
					const words = (
						<strong>{t("flaggedBy", { name, needs: row.needs })}</strong>
					);
					return (
						<li key={`task-${row.taskId}-${row.at}`} className={styles.row}>
							{one && <AgentPicture agent={one} />}
							<div className={styles.rowText}>
								{row.taskId ? (
									<Link to={`/tasks/${row.taskId}`}>{words}</Link>
								) : (
									words
								)}
								<span className={styles.muted}>{row.title}</span>
							</div>
						</li>
					);
				})}
			</ul>
		</section>
	);
}
