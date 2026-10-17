import { useState } from "react";
import { ShowAdvanced } from "../../components/ShowAdvanced.tsx";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import {
	Documents,
	Finish,
	Planning,
	RulesTitle,
	UiPaths,
	useRuleParts,
} from "../TeamRules.tsx";

/** How work runs: how finished work is added, planning in sprints, and the UI patterns. */
export function Work() {
	const { ready, part, key } = useRuleParts();
	const [advanced, setAdvanced] = useState(false);
	return (
		<div className={styles.page}>
			<RulesTitle name={t("sectionWork")} />
			{ready && (
				<>
					<Finish key={key("finish")} {...part("finish")} />
					<Planning key={key("planning")} {...part("planning")} />
					<Documents key={key("documents")} {...part("documents")} />
					<ShowAdvanced
						open={advanced}
						onToggle={() => setAdvanced(!advanced)}
						controls="work-advanced"
					/>
					{advanced && (
						<ul
							id="work-advanced"
							className={styles.tiers}
							aria-label={t("advanced")}
						>
							<li>
								<UiPaths key={key("paths")} {...part("paths")} />
							</li>
						</ul>
					)}
				</>
			)}
		</div>
	);
}
