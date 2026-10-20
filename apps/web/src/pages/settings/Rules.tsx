import { t } from "../../strings/t.ts";
import {
	Commands,
	ForbiddenFiles,
	Git,
	Limits,
	Remotes,
	SessionBudget,
} from "../GovernorRules.tsx";
import styles from "../pages.module.css";
import { RulesTitle, useRuleParts } from "../TeamRules.tsx";

/** What agents may touch and run: the governor's rules, each part saved on its own (ADR 0080). */
export function Rules() {
	const { ready, part, key } = useRuleParts();
	return (
		<div className={styles.page}>
			<RulesTitle name={t("sectionRules")} />
			{ready && <ForbiddenFiles key={key("files")} {...part("files")} />}
			{ready && <Commands key={key("commands")} {...part("commands")} />}
			{ready && <Git key={key("git")} {...part("git")} />}
			{ready && <Remotes key={key("remotes")} {...part("remotes")} />}
			{ready && <Limits key={key("limits")} {...part("limits")} />}
			{ready && <SessionBudget key={key("session")} {...part("session")} />}
		</div>
	);
}
