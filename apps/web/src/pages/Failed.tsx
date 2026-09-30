import { Link } from "react-router";
import { sentence } from "../app/words.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

/** A page whose reads were refused (a task that is not there, say): why, and the way home. */
export function Failed({ error }: { error: Error }) {
	return (
		<div className={styles.page}>
			<Link to="/">{t("backToToday")}</Link>
			<p role="alert" className={styles.alert}>
				{sentence(error.message)}
			</p>
		</div>
	);
}
