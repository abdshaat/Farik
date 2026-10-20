import mark from "@catervas/brand/assets/logo-mark-1254.png";
import wordmark from "@catervas/brand/assets/wordmark-1024.png";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

/** What shows while Catervas has not answered yet, so the page is never blank. */
export function Opening() {
	return (
		<main className={styles.connect}>
			<div className={styles.brand}>
				<img className={styles.mark} src={mark} alt="" />
				<img className={styles.wordmark} src={wordmark} alt={t("brand")} />
			</div>
			<p role="status">{t("openingCatervas")}</p>
		</main>
	);
}
