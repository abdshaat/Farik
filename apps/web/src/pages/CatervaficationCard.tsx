import { Button } from "@catervas/ui";
import { useId } from "react";
import { t } from "../strings/t.ts";
import styles from "./CatervaficationCard.module.css";
import { useCommand } from "./dialogs/StartSprint.tsx";

/** What `catervafication.get` answers: whether the offer stands, and the roles that would write. */
export type Catervafication = { offered: boolean; roles: string[] };

/**
 * The quiet card for a project the team has not documented yet: where "Later" left the offer, on
 * Today and on the Files page, until the owner starts it. `again` asks `catervafication.get` again.
 */
export function CatervaficationCard({ again }: { again: () => void }) {
	const title = useId();
	const { busy, refusal, send } = useCommand(again);
	return (
		<div className={styles.card}>
			<div className={styles.quiet}>
				<div>
					<strong id={title}>{t("cardTitle")}</strong>
					<p>{t("cardLead")}</p>
				</div>
				<Button
					busy={busy}
					aria-describedby={title}
					onClick={() => send({ command: "catervafication_start", body: {} })}
				>
					{t("readyStart")}
				</Button>
			</div>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
		</div>
	);
}
