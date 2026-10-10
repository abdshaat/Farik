import { useAdvanced } from "../../app/advanced.ts";
import styles from "../pages.module.css";
import { TeamRules } from "../TeamRules.tsx";

/** The team's rules on one page, until they have a page each. */
export function Rules() {
	const [advanced] = useAdvanced();
	return (
		<div className={styles.page}>
			<TeamRules advanced={advanced} />
		</div>
	);
}
