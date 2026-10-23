import { Button, Switch, TextField } from "@catervas/ui";
import { useEffect, useState } from "react";
import { useConnection, useHub } from "../app/connection.tsx";
import { commandSaid, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import styles from "./AllTeams.module.css";
import { FindingSettings } from "./find/FindingSettings.tsx";
import { GithubSettings } from "./find/GithubSettings.tsx";
import { dollars } from "./Gate.tsx";
import { useDigest } from "./reminders.ts";

type Computer = {
	agentsAtOnce: number | null;
	dailyCapUsd: number | null;
	spentTodayUsd: number;
};

/** The daily reminder: when it comes, and where its notice shows. */
function ReminderSettings() {
	const { client } = useHub();
	const { digest, again } = useDigest();
	const [said, setSaid] = useState<string>();
	if (!digest) return null;
	const set = async (change: object) => {
		setSaid(undefined);
		try {
			await client?.call("reminders.set", change);
		} catch (e) {
			setSaid(saidAll(e));
		}
		again();
	};
	return (
		<section className={styles.card} aria-labelledby="reminders-heading">
			<h2 id="reminders-heading">{t("remindersTitle")}</h2>
			<div>
				<label htmlFor="reminder-time">{t("remindersAt")}</label>{" "}
				<input
					id="reminder-time"
					type="time"
					value={digest.time}
					onChange={(e) => e.target.value && set({ time: e.target.value })}
				/>
			</div>
			<Switch
				id="reminder-browser"
				label={t("remindersBrowser")}
				description={t("remindersBrowserHint")}
				checked={digest.browser}
				onChange={(browser) => set({ browser })}
			/>
			<Switch
				id="reminder-system"
				label={t("remindersSystem")}
				description={t("remindersSystemHint")}
				checked={digest.system}
				onChange={(system) => set({ system })}
			/>
			{said && <p role="alert">{said}</p>}
		</section>
	);
}

/** This computer: the limits all the teams share (ADR 0059). */
export function ThisComputer() {
	const { client } = useConnection();
	const { data, again } = useQuery<Computer>("computer.get", {});
	const [agents, setAgents] = useState("");
	const [cap, setCap] = useState("");
	const [said, setSaid] = useState<string>();
	const [saved, setSaved] = useState(false);
	const [busy, setBusy] = useState(false);
	// Only the first answer fills the form; the owner's typing is not overwritten.
	const [filled, setFilled] = useState(false);
	useEffect(() => {
		if (!data || filled) return;
		setAgents(data.agentsAtOnce === null ? "" : String(data.agentsAtOnce));
		setCap(data.dailyCapUsd === null ? "" : String(data.dailyCapUsd));
		setFilled(true);
	}, [data, filled]);

	const save = async () => {
		if (!client) return;
		setBusy(true);
		setSaid(undefined);
		setSaved(false);
		try {
			await client.call("computer.set", {
				agentsAtOnce: agents === "" ? null : Number(agents),
				dailyCapUsd: cap.trim() === "" ? null : Number(cap),
			});
			setSaved(true);
			again();
		} catch (e) {
			const message = e instanceof Error ? e.message : "";
			setSaid(commandSaid(message, {}) || saidAll(e));
		} finally {
			setBusy(false);
		}
	};
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("computerPlace")}</h1>
			<form
				className={styles.card}
				onSubmit={(e) => {
					e.preventDefault();
					save();
				}}
			>
				<div>
					<label htmlFor="agents-at-once">{t("computerAgentsAtOnce")}</label>
					<p id="agents-at-once-hint" className={styles.muted}>
						{t("computerAgentsHint")}
					</p>
					<select
						id="agents-at-once"
						aria-describedby="agents-at-once-hint"
						value={agents}
						onChange={(e) => setAgents(e.target.value)}
					>
						<option value="">{t("computerNoLimit")}</option>
						{Array.from({ length: 16 }, (_, i) => i + 1).map((n) => (
							<option key={n} value={String(n)}>
								{n}
							</option>
						))}
					</select>
				</div>
				<TextField
					id="daily-limit"
					label={t("computerDailyLimit")}
					hint={t("computerDailyHint")}
					value={cap}
					onChange={setCap}
				/>
				{data && (
					<p>{t("computerToday", { amount: dollars(data.spentTodayUsd) })}</p>
				)}
				<div>
					<Button kind="primary" busy={busy} onClick={save}>
						{t("computerSave")}
					</Button>
				</div>
				{saved && <p role="status">{t("computerSaved")}</p>}
				{said && <p role="alert">{said}</p>}
			</form>
			<GithubSettings />
			<FindingSettings />
			<ReminderSettings />
		</div>
	);
}
