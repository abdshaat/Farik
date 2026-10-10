import { Button, TextArea } from "@catervas/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { saidAll } from "../../app/refusals.ts";
import { type ServeStatus, useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";
import { Wizard } from "./Wizard.tsx";

type Scan = {
	facts: {
		language: string | null;
		toolchain: string | null;
		workspace: boolean;
		packages: number;
		testsIn: string | null;
		lastCommit: string | null;
	};
	checks: string[];
	keptPrivate: string[];
};

/** "What it is": the language and toolchain, and the workspace when there is one. */
function what({ facts }: Scan): string {
	const parts = [facts.language, facts.toolchain].filter(Boolean);
	// One package is no workspace to speak of.
	if (facts.workspace && facts.packages > 1)
		parts.push(t("scanWorkspace").replace("{count}", String(facts.packages)));
	return parts.length ? parts.join(", ") : t("scanUnknown");
}

/** Setup's fourth step: the project scanned again, read back in rows. */
export function SetupScan() {
	const { client } = useConnection();
	const navigate = useNavigate();
	const { data: scan } = useQuery<Scan>("project.scan", {});
	const { data: serve } = useQuery<ServeStatus>("serve.status", {});
	const [wrong, setWrong] = useState(false);
	const [note, setNote] = useState("");
	const [saving, setSaving] = useState(false);
	const [saved, setSaved] = useState(false);
	const [refused, setRefused] = useState<string>();

	const send = async () => {
		if (!client) return;
		setSaving(true);
		setRefused(undefined);
		try {
			await client.call("project.note", { text: note.trim() });
			setSaved(true);
			setWrong(false);
			setNote("");
		} catch (e) {
			setRefused(saidAll(e));
		}
		setSaving(false);
	};

	return (
		<Wizard step={3} title={t("scanTitle")} lead={t("scanLead")}>
			{serve?.projectRoot && (
				<code className={styles.path}>{serve.projectRoot}</code>
			)}
			{scan && (
				<dl className={styles.facts} aria-label={t("scanFound")}>
					<div>
						<dt>{t("scanWhat")}</dt>
						<dd>{what(scan)}</dd>
					</div>
					<div>
						<dt>{t("scanTested")}</dt>
						<dd>
							{scan.facts.testsIn
								? t("scanTestsIn").replace("{location}", scan.facts.testsIn)
								: t("scanNoTests")}
						</dd>
					</div>
					<div>
						<dt>{t("scanChecked")}</dt>
						<dd>
							{scan.checks.length ? (
								<ul>
									{scan.checks.map((check) => (
										<li key={check}>{check}</li>
									))}
								</ul>
							) : (
								t("scanNoChecks")
							)}
						</dd>
					</div>
					<div>
						<dt>{t("scanLast")}</dt>
						<dd>{scan.facts.lastCommit ?? t("scanNoCommits")}</dd>
					</div>
					<div>
						<dt>{t("scanPrivate")}</dt>
						<dd>
							{scan.keptPrivate.length
								? `${scan.keptPrivate.join(", ")}. ${t("scanPrivateNote")}`
								: t("scanNothingPrivate")}
						</dd>
					</div>
				</dl>
			)}
			{scan && scan.checks.length > 0 && (
				<p className={styles.note}>{t("scanChecksNote")}</p>
			)}
			{saved && <p role="status">{t("scanWrongSaved")}</p>}
			{wrong && (
				<div className={styles.form}>
					<TextArea
						id="wrong"
						label={t("scanWrongField")}
						hint={t("scanWrongHint")}
						value={note}
						onChange={setNote}
						{...(refused && { error: refused })}
					/>
					<span>
						<Button busy={saving} disabled={!note.trim()} onClick={send}>
							{t("scanWrongSave")}
						</Button>
					</span>
				</div>
			)}
			<div className={styles.foot}>
				{wrong ? (
					<span />
				) : (
					<Button
						onClick={() => {
							setWrong(true);
							setSaved(false);
						}}
					>
						{t("scanWrong")}
					</Button>
				)}
				<Button kind="primary" onClick={() => navigate("/setup/team")}>
					{t("scanRight")}
				</Button>
			</div>
		</Wizard>
	);
}
