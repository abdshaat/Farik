import { Button } from "@catervas/ui";
import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { useQuery } from "../../app/store.ts";
import { ON_CURRENT } from "../../app/teams.ts";
import {
	base64Of,
	pendingSetupFiles,
	refusalWords,
} from "../../components/Attach.tsx";
import { ShowAdvanced } from "../../components/ShowAdvanced.tsx";
import { t } from "../../strings/t.ts";
import { AdvancedSettings } from "./SetupAdvanced.tsx";
import styles from "./setup.module.css";
import { useSetup, useStart } from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

/** Setup's last step: what happens to accepted work, said and not asked, then "Start the team". */
export function SetupFinish() {
	const navigate = useNavigate();
	const { draft } = useSetup();
	const { client } = useConnection();
	const { start, busy: starting, refused } = useStart();
	const [sending, setSending] = useState(false);
	const [notAttached, setNotAttached] = useState<string>();
	const busy = starting || sending;
	// The documents picked on the project screen go first, one by one; a refused one is said, and the team starts anyway.
	const begin = async () => {
		if (!client) return;
		setSending(true);
		setNotAttached(undefined);
		while (pendingSetupFiles.length > 0) {
			const next = pendingSetupFiles.shift();
			if (!next) break;
			try {
				await client.call("document.attach", {
					name: next.file.name,
					kind: next.kind,
					dataBase64: await base64Of(next.file),
				});
			} catch (e) {
				setNotAttached(refusalWords(e));
			}
		}
		setSending(false);
		await start();
	};
	const [advanced, setAdvanced] = useState(false);
	// Whether finished work can open a pull request depends on the project having an origin.
	const { data: team } = useQuery<{ online?: boolean }>("team.get", {});
	const said = team
		? t(team.online ? "finishLead" : "finishLeadLocal")
		: undefined;

	return (
		<Wizard
			step={7}
			title={t("finishTitle")}
			lead={
				draft.from && said
					? `${said} ${t("finishLeadSaved", { name: draft.from })}`
					: (said ?? "")
			}
			carried={!!draft.from}
		>
			{draft.from && <Carried from={draft.from} answers={draft.answers} />}
			<p className={styles.note}>{t("finishSafe")}</p>
			<span>
				<ShowAdvanced
					open={advanced}
					onToggle={() => setAdvanced(!advanced)}
					controls="finish-advanced"
				/>
			</span>
			{advanced && (
				<div id="finish-advanced">
					<AdvancedSettings />
				</div>
			)}
			{notAttached && (
				<p role="alert" className={styles.alert}>
					{notAttached}
				</p>
			)}
			{refused && (
				<p role="alert" className={styles.alert}>
					{refused}
				</p>
			)}
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/spending")}>{t("back")}</Button>
				<Button kind="primary" busy={busy} onClick={begin}>
					{t("startTeam")}
				</Button>
			</div>
		</Wizard>
	);
}

/** A saved team's two permission answers, in words, since setup does not ask them again. */
function Carried({
	from,
	answers,
}: {
	from: string;
	answers: { commands?: boolean; push?: boolean };
}) {
	const rows = [
		[
			"finishCommands",
			answers.commands,
			"finishCommandsYes",
			"finishCommandsNo",
		],
		["finishPush", answers.push, "finishPushYes", "finishPushNo"],
	] as const;
	return (
		<section className={styles.card} aria-labelledby="carried">
			<h2 id="carried" className={styles.heading}>
				{t("finishCarried", { name: from })}
			</h2>
			<dl className={styles.facts}>
				{rows.map(([what, yes, said, not]) => (
					<div key={what}>
						<dt>{t(what)}</dt>
						<dd>
							<strong>{t(yes ? "finishYes" : "finishNo")}</strong>{" "}
							{t(yes ? said : not)}
						</dd>
					</div>
				))}
			</dl>
			<p>
				{t("finishCarriedBefore")}
				<Link to="/settings" state={ON_CURRENT}>
					{t("settings")}
				</Link>
				{t("finishCarriedAfter")}
			</p>
		</section>
	);
}
