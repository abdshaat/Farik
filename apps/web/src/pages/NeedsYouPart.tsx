import { t } from "../strings/t.ts";
import styles from "./NeedsYou.module.css";
import { ReminderItem } from "./ReminderItem.tsx";
import { useMine } from "./reminders.ts";
import { useReminderActions } from "./useReminderActions.tsx";

/** The part of the daily digest that is this team's, on its Today; nothing while it has none. */
export function NeedsYouPart() {
	const { mine, again } = useMine();
	const { dialog, onGive, onSnooze, onDismiss } = useReminderActions(again);
	if (!mine || mine.items.length === 0) return null;
	return (
		<section className={styles.group} aria-labelledby="needs-you-part">
			<h2 id="needs-you-part">{t("needsYouTitle")}</h2>
			<ul className={styles.list}>
				{mine.items.map((item) => (
					<ReminderItem
						key={item.key}
						item={item}
						onGive={onGive}
						onSnooze={onSnooze}
						onDismiss={onDismiss}
					/>
				))}
			</ul>
			{dialog}
		</section>
	);
}
