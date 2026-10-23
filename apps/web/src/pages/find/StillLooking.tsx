import { Link } from "react-router";
import { t } from "../../strings/t.ts";
import styles from "./find.module.css";
import { counted, useFindingStatus } from "./finding.ts";

/** The strip on every setup page while a scan runs, with a way to what is found so far. */
export function StillLooking() {
	const { status } = useFindingStatus();
	if (status?.phase !== "running") return null;
	return (
		<p role="status" className={styles.strip}>
			{counted(status.projects, t("stillLookingOne"), (count) =>
				t("stillLooking", { count }),
			)}
			<Link to="/find/found">{t("stillLookingSee")}</Link>
		</p>
	);
}
