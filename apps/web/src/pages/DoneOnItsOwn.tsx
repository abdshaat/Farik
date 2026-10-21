import { Button, RoleTag } from "@catervas/ui";
import { useState } from "react";
import { AgentPicture } from "../components/AgentPicture.tsx";
import { t } from "../strings/t.ts";
import type { Agent } from "./setup/TeamSetup.tsx";
import styles from "./Today.module.css";

export type DoneRow = {
	at: string;
	line: string;
	agentId?: string;
	taskId?: string;
	reason?: string;
};

/** How many acts show at first; the rest are behind "Show all". */
const SHOWN = 20;

/**
 * What the team did without asking in the last seven days (ADR 0082), newest first: the time
 * (HH:MM UTC, with the day when it is not today), who did it, the line, and why when it says.
 */
export function DoneOnItsOwn({
	rows,
	agents,
	today,
}: {
	rows: DoneRow[];
	agents: Agent[];
	today: Date;
}) {
	const [all, setAll] = useState(false);
	const day = today.toISOString().slice(0, 10);
	const newest = [...rows].sort((a, b) => b.at.localeCompare(a.at));
	const folds = newest.length > SHOWN;
	const shown = folds && !all ? newest.slice(0, SHOWN) : newest;
	return (
		<section className={styles.section} aria-labelledby="done-heading">
			<h2 id="done-heading">{t("doneTitle")}</h2>
			{rows.length === 0 ? (
				<p className={styles.muted}>{t("doneNone")}</p>
			) : (
				<>
					<ul
						id="done-list"
						className={styles.moved}
						aria-label={t("doneTitle")}
					>
						{shown.map((row) => {
							const one = agents.find((a) => a.id === row.agentId);
							const hhmm = row.at.slice(11, 16);
							const when =
								row.at.slice(0, 10) === day
									? hhmm
									: `${row.at.slice(0, 10)} ${hhmm}`;
							return (
								<li key={`${row.at}-${row.agentId}-${row.line}`}>
									{one && <AgentPicture agent={one} size={48} />}
									<div className={styles.movedText}>
										<span className={styles.movedHead}>
											{one && (
												<>
													<strong>{one.displayName}</strong>
													<RoleTag role={one.role} />
												</>
											)}
											<time dateTime={row.at}>{when}</time>
										</span>
										<span>{row.line}</span>
										{row.reason && (
											<span className={styles.muted}>
												{t("doneWhy", { reason: row.reason })}
											</span>
										)}
									</div>
								</li>
							);
						})}
					</ul>
					{folds && !all && (
						<div className={styles.movedMore}>
							<Button
								kind="quiet"
								expanded={false}
								controls="done-list"
								onClick={() => setAll(true)}
							>
								{t("doneShowAll", { count: newest.length })}
							</Button>
						</div>
					)}
				</>
			)}
		</section>
	);
}
