import { Button } from "@catervas/ui";
import { useState } from "react";
import { Link, useNavigate } from "react-router";
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
	const { start, busy, refused } = useStart();
	const [advanced, setAdvanced] = useState(false);

	return (
		<Wizard
			step={7}
			title={t("finishTitle")}
			lead={
				draft.from
					? t("finishLeadSaved", { name: draft.from })
					: t("finishLead")
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
			{refused && (
				<p role="alert" className={styles.alert}>
					{refused}
				</p>
			)}
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/spending")}>{t("back")}</Button>
				<Button kind="primary" busy={busy} onClick={start}>
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
				<Link to="/settings">{t("settings")}</Link>
				{t("finishCarriedAfter")}
			</p>
		</section>
	);
}
