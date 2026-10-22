import { Button } from "@catervas/ui";
import { useState } from "react";
import { useHub } from "../../app/connection.tsx";
import { t } from "../../strings/t.ts";
import { DropZone } from "./ExportDrop.tsx";
import styles from "./find.module.css";
import {
	EXPORT_PENDING,
	findingSaid,
	keep,
	kept,
	useFindingStatus,
} from "./finding.ts";

/**
 * The card that waits for claude.ai's export while the owner chose "I'll do it later": on All teams
 * and on the found list, until a file is brought in or the owner dismisses it.
 */
export function ExportCard() {
	const { client } = useHub();
	const { status } = useFindingStatus();
	const [pending, setPending] = useState(kept(EXPORT_PENDING) !== undefined);
	const [said, setSaid] = useState<string>();
	if (!pending || status === undefined || status.exportDismissed) return null;
	const dismiss = async () => {
		setSaid(undefined);
		try {
			await client?.call("finding.export_dismissed", {});
			keep(EXPORT_PENDING, undefined);
			setPending(false);
		} catch (e) {
			setSaid(findingSaid(e, "refuseOther"));
		}
	};
	return (
		<section className={styles.card} aria-labelledby="export-card">
			<h2 id="export-card">{t("exportCardTitle")}</h2>
			<p>{t("exportCardNote")}</p>
			<DropZone compact />
			<div className={styles.actions}>
				<Button onClick={dismiss}>{t("exportDismiss")}</Button>
			</div>
			{said && <p role="alert">{said}</p>}
		</section>
	);
}
