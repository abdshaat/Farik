import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { RulesTitle, Spend, useRuleParts } from "../TeamRules.tsx";

export function Spending() {
	const { ready, part, key } = useRuleParts();
	return (
		<div className={styles.page}>
			<RulesTitle name={t("sectionSpending")} />
			{ready && <Spend key={key("spend")} {...part("spend")} />}
		</div>
	);
}
