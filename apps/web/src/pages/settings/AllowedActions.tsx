import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { May, RulesTitle, useRuleParts } from "../TeamRules.tsx";

export function AllowedActions() {
	const { ready, part, key } = useRuleParts();
	return (
		<div className={styles.page}>
			<RulesTitle name={t("sectionAllowedActions")} />
			{ready && <May key={key("may")} {...part("may")} />}
		</div>
	);
}
