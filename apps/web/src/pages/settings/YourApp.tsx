import { Link } from "../../app/links.tsx";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { HowToOpen } from "../TeamRules.tsx";

/** Your app: how to open it, or what to add first when the team has no UI/UX Designer. */
export function YourApp() {
	return (
		<div className={styles.page}>
			<HowToOpen
				none={
					<p>
						{t("yourAppNone")} <Link to="/team">{t("team")}</Link>
					</p>
				}
			/>
		</div>
	);
}
