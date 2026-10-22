import { Button, Dialog } from "@catervas/ui";
import { t } from "../strings/t.ts";
import styles from "./NeedsYou.module.css";
import type { ReminderItemData } from "./reminders.ts";

/** Giving an item to the team of its project: the request that is filed, and a button that files it. */
export function GiveToTeam({
	item,
	busy,
	said,
	onClose,
	onSend,
}: {
	item: ReminderItemData;
	busy: boolean;
	said: string | undefined;
	onClose: () => void;
	onSend: () => void;
}) {
	const team = item.give?.teamName ?? "the team";
	return (
		<Dialog
			open
			title={t("needsYouGiveTitle", { team })}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("needsYouCancel")}</Button>
					<Button kind="primary" busy={busy} onClick={onSend}>
						{t("needsYouSend")}
					</Button>
				</>
			}
		>
			<p className={styles.label}>{t("needsYouGiveRequest")}</p>
			<p className={styles.muted}>{t("needsYouGiveWho")}</p>
			<p className={styles.request}>{item.give?.text}</p>
			{said && <p role="alert">{said}</p>}
		</Dialog>
	);
}
