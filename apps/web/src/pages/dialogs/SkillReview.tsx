import { Button, Dialog } from "@catervas/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { said } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { kb, type SkillAt, sendSkill, useSkillFolder } from "../skills.ts";
import { FileFrame, inOrder } from "./FileFrame.tsx";

const WHY: Record<string, Parameters<typeof t>[0]> = {
	skill_runs_commands: "skillWhyRuns",
	skill_attaches_files: "skillWhyAttaches",
	skill_too_large: "skillWhyLarge",
	skill_file_not_text: "skillWhyNotText",
};

/** A skill that came with the project, read whole before it is used (spec 6.7). */
export function SkillReview({
	agent,
	name: who,
	role,
	skill,
	shipped,
	onClose,
}: {
	agent: string;
	/** The agent's name. */
	name: string;
	role: string;
	skill: SkillAt;
	shipped: string[];
	/** `changed` once the skill was confirmed or removed. */
	onClose: (changed: boolean) => void;
}) {
	const { client } = useConnection();
	const got = useSkillFolder(agent, skill);
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	// A name another role ships is learned from the daemon's refusal; the person then chooses.
	const [taken, setTaken] = useState(false);
	const ours = shipped.includes(skill.name);
	const replaces = ours || taken;
	const at = { level: skill.level, ...(skill.level === "agent" && { agent }) };
	const send = async (command: string, body: object) => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		const code = await sendSkill(client, { command, body: { ...at, ...body } });
		setBusy(false);
		if (code === undefined) onClose(true);
		else if (code === "skill_name_taken") setTaken(true);
		else setRefusal(said(code, {}, "skillOtherRefusal"));
	};
	const remove = () => send("skill_remove", { name: skill.name });
	const refused = got && "refused" in got;
	return (
		<Dialog
			open
			fillsPhone
			title={t("skillReviewTitle", { skill: skill.name })}
			onClose={() => onClose(false)}
			actions={
				<>
					<Button busy={busy} onClick={remove}>
						{t("skillRemoveButton")}
					</Button>
					{got && !refused && (
						<Button
							kind="primary"
							busy={busy}
							onClick={() =>
								send("skill_confirm", {
									name: skill.name,
									sha256: got.sha256,
									...(replaces && { replaceShipped: true }),
								})
							}
						>
							{t("skillReviewUse")}
						</Button>
					)}
				</>
			}
		>
			<div className={styles.toolApproval}>
				<p>{t("skillReviewWhy", { name: who })}</p>
				{!got && <p className={styles.muted}>{t("skillReviewReading")}</p>}
				{refused && (
					<p role="alert" className={styles.alert}>
						{t("skillCannotUse", {
							reason: t(WHY[got.refused] ?? "skillWhyOther"),
						})}
					</p>
				)}
				{got && !refused && (
					<>
						<h3>{t("skillReviewWhole")}</h3>
						<p className={styles.toolHint}>{t("skillReviewShown")}</p>
						{inOrder(got.files).map(([path, text]) => (
							<FileFrame key={path} path={path} text={text} />
						))}
						<h3>{t("skillReviewFiles")}</h3>
						<ul>
							{Object.entries(got.files).map(([path, text]) => (
								<li key={path}>
									<code>{path}</code>, {t("skillSize", { kb: kb(text) })}
								</li>
							))}
						</ul>
						{replaces && (
							<p>
								<strong>
									{t(ours ? "skillReplaces" : "skillReplacesCatervas", {
										role,
										skill: skill.name,
										whom: skill.level === "agent" ? who : t("skillWholeTeam"),
									})}
								</strong>
							</p>
						)}
					</>
				)}
				{refusal && !refused && (
					<p role="alert" className={styles.alert}>
						{refusal}
					</p>
				)}
			</div>
		</Dialog>
	);
}
