import { Button, Choice, Dialog, TextField } from "@farik/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { commandSaid } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";

/** Sends one command; closes on success, else keeps its refusal in words. */
export function useCommand(onDone: () => void) {
	const { client } = useConnection();
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const send = async (command: object) => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const reply = await client.command(command as never);
			if ("error" in reply) setRefusal(commandSaid(reply.error.detail, {}));
			else onDone();
		} catch {
			// The connection closed: the page shows that it is lost.
		} finally {
			setBusy(false);
		}
	};
	return { busy, refusal, send };
}

/** One row waiting in the Backlog; `parts` is an epic's task count. */
export type Waiting = {
	taskId: string;
	title: string;
	parts?: number | undefined;
};

/**
 * "Start sprint N": who plans it, and an optional spending limit; under the policy, what waits in
 * the Backlog (step 15).
 */
export function StartSprint({
	n,
	planner,
	ready,
	backlog,
	onClose,
}: {
	n: string;
	planner: string;
	ready: number;
	/** What waits in the Backlog, when the team plans in sprints. */
	backlog?: Waiting[] | undefined;
	onClose: () => void;
}) {
	const [limit, setLimit] = useState<"none" | "limit">("none");
	const [amount, setAmount] = useState("20.00");
	const { busy, refusal, send } = useCommand(onClose);
	const title = t("sprintStartTitle").replace("{n}", n);
	const usd = Number(amount);
	return (
		<Dialog
			open
			title={title}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("sprintNotNow")}</Button>
					<Button
						kind="primary"
						busy={busy}
						disabled={limit === "limit" && !(usd > 0)}
						onClick={() =>
							send({
								command: "sprint_start",
								body: { budgetUsd: limit === "limit" ? usd : null },
							})
						}
					>
						{title}
					</Button>
				</>
			}
		>
			<p className={styles.muted}>{t("sprintStartWhat")}</p>
			{backlog && (
				<section aria-labelledby="sprint-backlog">
					<h3 id="sprint-backlog" className={styles.subheading}>
						{t("sprintStartBacklog")}
					</h3>
					<ul className={styles.waiting} aria-labelledby="sprint-backlog">
						{backlog.map((w) => (
							<li key={w.taskId}>
								<span className={styles.muted}>{w.taskId}</span>
								<span>{w.title}</span>
								<span className={styles.muted}>
									{w.parts === undefined
										? t("sprintStartTask")
										: w.parts === 1
											? t("sprintStartEpicOne")
											: t("sprintStartEpic", { count: w.parts })}
								</span>
							</li>
						))}
					</ul>
				</section>
			)}
			<p>
				{backlog
					? t("sprintStartPlansBacklog", { name: planner })
					: t("sprintStartPlanner")
							.replaceAll("{name}", planner)
							.replace("{count}", String(ready))}
			</p>
			<Choice
				name="sprint-limit"
				legend={t("sprintLimit")}
				value={limit}
				onChange={setLimit}
				options={[
					{ value: "none", label: t("sprintNoLimit") },
					{ value: "limit", label: t("sprintLimitAfter") },
				]}
			/>
			{limit === "limit" && (
				<TextField
					id="sprint-limit-usd"
					label={t("sprintLimitDollars")}
					value={amount}
					onChange={setAmount}
				/>
			)}
			<p className={styles.muted}>{t("sprintLimitHint")}</p>
			{refusal && <p role="alert">{refusal}</p>}
		</Dialog>
	);
}
