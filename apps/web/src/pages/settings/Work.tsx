import { useAdvanced } from "../../app/advanced.ts";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import {
	Finish,
	Planning,
	Plans,
	RulesTitle,
	UiPaths,
	useRuleParts,
} from "../TeamRules.tsx";

/** How work runs: how finished work is added, planning in sprints, the plan check, and the UI patterns. */
export function Work() {
	const { ready, part, key } = useRuleParts();
	const [advanced] = useAdvanced();
	return (
		<div className={styles.page}>
			<RulesTitle name={t("sectionWork")} />
			{ready && (
				<>
					<Finish key={key("finish")} {...part("finish")} />
					<Planning key={key("planning")} {...part("planning")} />
					<Plans key={key("plans")} {...part("plans")} />
					{advanced && <UiPaths key={key("paths")} {...part("paths")} />}
				</>
			)}
		</div>
	);
}
