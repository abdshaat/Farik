import { Button, Dialog, Switch } from "@catervas/ui";
import { useState } from "react";
import { useHub } from "../../app/connection.tsx";
import { t } from "../../strings/t.ts";
import styles from "./find.module.css";
import {
	type Choice,
	type FoundRow,
	findingSaid,
	useFindingStatus,
	useHubCall,
} from "./finding.ts";

/** When the last scan ended, as the owner says it. */
function lastLooked(endedAt: string, now = new Date()): string {
	const at = new Date(endedAt);
	const time = at.toLocaleTimeString("en-GB", {
		hour: "2-digit",
		minute: "2-digit",
	});
	const midnight = new Date(now).setHours(0, 0, 0, 0);
	const days = Math.ceil((midnight - at.getTime()) / 86_400_000);
	if (days <= 0) return t("findingLastToday", { time });
	if (days === 1) return t("findingLastYesterday", { time });
	return t("findingLastDays", { count: days });
}

/** This computer's finding section (docs/design/mockups/ManyFixMistake): the daily look, the ignored, the longer history. */
export function FindingSettings() {
	const { client } = useHub();
	const { status, again: askStatus } = useFindingStatus();
	const ignored = useHubCall<{ ignored: FoundRow[] }>("finding.ignored");
	const retention = useHubCall<{ cleanupPeriodDays: number; default: boolean }>(
		"finding.retention",
	);
	const [bringing, setBringing] = useState<string>();
	const [forgetting, setForgetting] = useState(false);
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();

	/** Calls `method` and asks for everything again; says why when it is refused. */
	const call = async (
		method:
			| "finding.pause"
			| "finding.start"
			| "finding.choose"
			| "finding.keep_longer"
			| "finding.forget",
		params: object,
	): Promise<boolean> => {
		setBusy(true);
		setSaid(undefined);
		try {
			await client?.call(method, params);
			askStatus();
			ignored.again();
			retention.again();
			return true;
		} catch (e) {
			setSaid(findingSaid(e, "findingFailed"));
			return false;
		} finally {
			setBusy(false);
		}
	};

	if (!status) return null;
	const paused = status.paused;
	const days = retention.data?.cleanupPeriodDays;
	return (
		<>
			<section className={styles.card} aria-labelledby="finding-title">
				<h2 id="finding-title">{t("findingTitle")}</h2>
				<Switch
					id="finding-daily"
					label={t("findingDaily")}
					checked={!paused}
					{...(status.endedAt && { description: lastLooked(status.endedAt) })}
					onChange={(on) => void call("finding.pause", { paused: !on })}
				/>
				<div className={styles.actions}>
					<Button
						busy={status.phase === "running"}
						disabled={paused}
						onClick={() => void call("finding.start", {})}
					>
						{t("findingScanNow")}
					</Button>
				</div>
				{ignored.data && ignored.data.ignored.length > 0 && (
					<div>
						<h3>{t("findingIgnored")}</h3>
						<ul className={styles.list}>
							{ignored.data.ignored.map((row) => (
								<li key={row.key} className={styles.row}>
									<span>{row.name}</span>
									{bringing === row.key ? (
										<span className={styles.actions}>
											{(["team", "watch"] as Choice[])
												.filter(
													(choice) =>
														choice !== "team" ||
														(row.kind !== "claude_ai" && row.kind !== "job"),
												)
												.map((choice) => (
													<Button
														key={choice}
														onClick={async () => {
															if (
																await call("finding.choose", {
																	key: row.key,
																	choice,
																})
															)
																setBringing(undefined);
														}}
													>
														{choice === "team"
															? t("foundChoiceTeam")
															: t("foundChoiceWatch")}
													</Button>
												))}
										</span>
									) : (
										<Button onClick={() => setBringing(row.key)}>
											{t("findingBringBack")}
										</Button>
									)}
								</li>
							))}
						</ul>
					</div>
				)}
				{retention.data?.default && (
					<div>
						<h3>{t("findingKeepTitle")}</h3>
						<ul>
							<li>{t("findingKeepDeletes", { days: days ?? 30 })}</li>
							<li>
								{t("findingKeepChange")} <code>{t("findingKeepLine")}</code>
							</li>
							<li>{t("findingKeepRest")}</li>
						</ul>
						<p className={styles.muted}>{t("findingKeepThen")}</p>
						<div className={styles.actions}>
							<Button
								onClick={() => void call("finding.keep_longer", { days: 365 })}
							>
								{t("findingKeepYear")}
							</Button>
						</div>
					</div>
				)}
				{said && <p role="alert">{said}</p>}
			</section>
			<section className={styles.card} aria-labelledby="forget-title">
				<h2 id="forget-title">{t("findingForgetTitle")}</h2>
				<p>
					<strong>{t("findingForgetYes")}</strong> {t("findingForgetWhat")}
				</p>
				<div className={styles.actions}>
					<Button onClick={() => setForgetting(true)}>
						{t("findingForgetAsk")}
					</Button>
				</div>
			</section>
			{forgetting && (
				<Dialog
					open
					title={t("findingForgetDialog")}
					onClose={() => setForgetting(false)}
					actions={
						<>
							<Button onClick={() => setForgetting(false)}>
								{t("agentCancel")}
							</Button>
							<Button
								kind="primary"
								busy={busy}
								onClick={async () => {
									if (await call("finding.forget", { confirmed: true }))
										setForgetting(false);
								}}
							>
								{t("findingForgetYes")}
							</Button>
						</>
					}
				>
					<ul>
						<li>{t("findingForgetDeletes")}</li>
						<li>{t("findingForgetStays")}</li>
						<li>{t("findingForgetPauses")}</li>
					</ul>
					{said && <p role="alert">{said}</p>}
				</Dialog>
			)}
		</>
	);
}
