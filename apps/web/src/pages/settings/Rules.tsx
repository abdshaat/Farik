import { t } from "../../strings/t.ts";
import { ForbiddenFiles } from "../GovernorRules.tsx";
import styles from "../pages.module.css";
import { RulesTitle, useRuleParts } from "../TeamRules.tsx";

/** What agents may touch and run: the governor's rules, each part saved on its own (ADR 0080). */
export function Rules() {
	const { ready, part, key } = useRuleParts();
	return (
		<div className={styles.page}>
			<RulesTitle name={t("sectionRules")} />
			{ready && <ForbiddenFiles key={key("files")} {...part("files")} />}
		</div>
	);
}
