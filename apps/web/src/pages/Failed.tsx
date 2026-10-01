import { RpcError } from "@farik/protocol-client";
import { Link } from "react-router";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

/** The daemon's code for something that is not there (`NOT_FOUND`). */
const NOT_FOUND = -32002;

/** A page whose reads were refused (a task that is not there, say): why, in plain words, and the way home. */
export function Failed({ error }: { error: Error }) {
	const gone = error instanceof RpcError && error.code === NOT_FOUND;
	return (
		<div className={styles.page}>
			<Link to="/">{t("backToToday")}</Link>
			<p role="alert" className={styles.alert}>
				{t(gone ? "pageNotFound" : "pageFailed")}
			</p>
		</div>
	);
}
