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

/** "Start sprint N": who plans it, and an optional spending limit. */
export function StartSprint({
	n,
	planner,
	ready,
	onClose,
}: {
	n: string;
	planner: string;
	ready: number;
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
			<p>
				{t("sprintStartPlanner")
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
