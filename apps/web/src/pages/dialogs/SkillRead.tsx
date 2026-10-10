import { Button, Dialog } from "@catervas/ui";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { type SkillShipped, useSkillFolder } from "../skills.ts";
import { inOrder } from "./FileFrame.tsx";

/**
 * A skill that comes with a role, read-only (spec 6.7). Catervas ships it, pinned in the program,
 * so its text is shown as written and not in an `untrusted` frame.
 */
export function SkillRead({
	agent,
	skill,
	onClose,
}: {
	agent: string;
	skill: SkillShipped;
	onClose: () => void;
}) {
	const got = useSkillFolder(agent, skill);
	return (
		<Dialog
			open
			title={t("skillReadTitle", { skill: skill.name })}
			onClose={onClose}
			actions={<Button onClick={onClose}>{t("skillClose")}</Button>}
		>
			<div className={styles.toolApproval}>
				<p>{t("skillReadNote")}</p>
				{!got && <p className={styles.muted}>{t("skillReviewReading")}</p>}
				{got && "refused" in got && (
					<p role="alert" className={styles.alert}>
						{t("skillCannotOpen", { skill: skill.name })}
					</p>
				)}
				{got &&
					!("refused" in got) &&
					inOrder(got.files).map(([path, text]) => (
						<section key={path} aria-label={path}>
							<p className={styles.toolHint}>
								<code>{path}</code>
							</p>
							<pre className={styles.code}>{text}</pre>
						</section>
					))}
			</div>
		</Dialog>
	);
}
