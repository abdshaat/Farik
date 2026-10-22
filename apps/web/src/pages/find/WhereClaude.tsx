import { useNavigate } from "react-router";
import { t } from "../../strings/t.ts";
import { Wizard } from "../setup/Wizard.tsx";
import { useStartLooking } from "./FindOffer.tsx";
import styles from "./find.module.css";

/** "Where do you use Claude?": the computer check found no Claude Code history (journey C). */
export function WhereClaude() {
	const navigate = useNavigate();
	const looking = useStartLooking(true);
	return (
		<Wizard step={0} title={t("whereTitle")} lead={t("whereLead")}>
			<div className={styles.options}>
				<button
					type="button"
					className={styles.option}
					disabled={looking.busy}
					onClick={looking.start}
				>
					<strong>{t("whereCode")}</strong>
					<span>{t("whereCodeNote")}</span>
				</button>
				<button
					type="button"
					className={styles.option}
					onClick={() => navigate("/setup/account")}
				>
					<strong>{t("whereNew")}</strong>
					<span>{t("whereNewNote")}</span>
				</button>
			</div>
			{looking.said && <p role="alert">{looking.said}</p>}
		</Wizard>
	);
}
