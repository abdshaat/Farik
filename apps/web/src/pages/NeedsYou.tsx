import { Button } from "@catervas/ui";
import { useState } from "react";
import { t } from "../strings/t.ts";
import { keep, kept } from "./find/finding.ts";
import styles from "./NeedsYou.module.css";
import { ReminderItem } from "./ReminderItem.tsx";
import { type GroupName, useDigest } from "./reminders.ts";
import { useReminderActions } from "./useReminderActions.tsx";

const HEADINGS: Record<Exclude<GroupName, "runs">, Parameters<typeof t>[0]> = {
	forgotten: "needsYouForgotten",
	ending: "needsYouEnding",
	schedules: "needsYouSchedules",
	teams: "needsYouTeams",
	services: "needsYouServices",
};

/** Kept in this browser once the owner said "Not now" to the offer of notices. */
const NOT_NOW = "catervas.noticesNotNow";

/** Asks whether the browser may show notices, while it has not been asked and the owner did not decline. */
function NoticeOffer() {
	const [permission, setPermission] = useState(() =>
		typeof Notification === "undefined"
			? "unsupported"
			: Notification.permission,
	);
	const [declined, setDeclined] = useState(() => kept(NOT_NOW) === "1");
	if (permission !== "default" || declined) return null;
	return (
		<div className={styles.offer}>
			<p>{t("needsYouNoticeOffer")}</p>
			<Button
				kind="primary"
				onClick={async () =>
					setPermission(await Notification.requestPermission())
				}
			>
				{t("needsYouAllow")}
			</Button>
			<Button
				kind="quiet"
				onClick={() => {
					keep(NOT_NOW, "1");
					setDeclined(true);
				}}
			>
				{t("needsYouNotNow")}
			</Button>
		</div>
	);
}

/** Needs you today: the daily digest of what the owner may have forgotten, with each item's actions. */
export function NeedsYou() {
	const { digest, again } = useDigest();
	const { dialog, onGive, onSnooze, onDismiss } = useReminderActions(again);
	const runs = digest?.groups.find((g) => g.group === "runs")?.items ?? [];
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("needsYouTitle")}</h1>
			{digest && (
				<p className={styles.muted}>
					{t(digest.count === 1 ? "needsYouCountOne" : "needsYouCount", {
						count: digest.count,
						time: digest.time,
					})}
				</p>
			)}
			<NoticeOffer />
			{digest && digest.count === 0 && <p>{t("needsYouNothing")}</p>}
			{digest?.groups
				.filter(
					(group): group is typeof group & { group: keyof typeof HEADINGS } =>
						group.group !== "runs",
				)
				.map(({ group, items }) => (
					<section
						key={group}
						className={styles.group}
						aria-labelledby={`needs-${group}`}
					>
						<h2 id={`needs-${group}`}>{t(HEADINGS[group])}</h2>
						<ul className={styles.list}>
							{items.map((item) => (
								<ReminderItem
									key={item.key}
									item={item}
									onGive={onGive}
									onSnooze={onSnooze}
									onDismiss={onDismiss}
								/>
							))}
						</ul>
					</section>
				))}
			{runs.length > 0 && (
				<details className={styles.runs}>
					<summary>{t("needsYouRuns", { count: runs.length })}</summary>
					<ul className={styles.list}>
						{runs.map((item) => (
							<ReminderItem
								key={item.key}
								item={item}
								onGive={onGive}
								onSnooze={onSnooze}
								onDismiss={onDismiss}
							/>
						))}
					</ul>
				</details>
			)}
			{dialog}
		</div>
	);
}
