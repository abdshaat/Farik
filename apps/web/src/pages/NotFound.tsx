import { Link } from "react-router";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

export function NotFound() {
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("noPage")}</h1>
			<Link to="/">{t("noPageHome")}</Link>
		</div>
	);
}
