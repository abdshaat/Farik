import { Button, InfoTip } from "@catervas/ui";
import { useState } from "react";
import { Link } from "../app/links.tsx";
import { t } from "../strings/t.ts";
import styles from "./NeedsYou.module.css";
import type { ReminderItemData } from "./reminders.ts";

/** What an item that needs explaining says behind its info button. */
function Explain({ item }: { item: ReminderItemData }) {
	if (!item.explain) return null;
	const [one, two] =
		item.explain === "cron"
			? (["needsYouExplainCronOne", "needsYouExplainCronTwo"] as const)
			: (["needsYouExplainOffOne", "needsYouExplainOffTwo"] as const);
	return (
		<InfoTip id={`explain-${item.key}`} label={t("needsYouMore")}>
			<ul>
				<li>{t(one)}</li>
				<li>{t(two)}</li>
			</ul>
		</InfoTip>
	);
}

/** One item of the digest with the actions it offers. */
export function ReminderItem({
	item,
	onGive,
	onSnooze,
	onDismiss,
}: {
	item: ReminderItemData;
	onGive: (item: ReminderItemData) => void;
	onSnooze: (item: ReminderItemData, length: "day" | "week") => void;
	onDismiss: (item: ReminderItemData) => void;
}) {
	const [snoozing, setSnoozing] = useState(false);
	const [copied, setCopied] = useState(false);
	const copy = async (command: string) => {
		try {
			await navigator.clipboard.writeText(command);
			setCopied(true);
		} catch {}
	};
	const hideable = item.kind !== "team" && item.kind !== "run";
	return (
		<li className={styles.item}>
			<p className={styles.itemTitle}>
				{item.title}
				<Explain item={item} />
			</p>
			{item.detail && <p className={styles.muted}>{item.detail}</p>}
			{item.kind !== "run" && (
				<div className={styles.actions}>
					{item.resume && (
						<Button onClick={() => copy(item.resume?.command ?? "")}>
							{t("needsYouCopyResume")}
						</Button>
					)}
					{copied && <span role="status">{t("needsYouCopied")}</span>}
					{item.claudeAiUrl && (
						<a
							href={item.claudeAiUrl}
							target="_blank"
							rel="noopener noreferrer"
						>
							{t("needsYouOpenClaudeAi")}
						</a>
					)}
					{item.githubUrl && (
						<a href={item.githubUrl} target="_blank" rel="noopener noreferrer">
							{t("needsYouOpenGithub")}
						</a>
					)}
					{item.give && (
						<Button onClick={() => onGive(item)}>{t("needsYouGive")}</Button>
					)}
					{item.answer && (
						<Link to={`/t/${item.answer.team}/`}>{t("needsYouAnswer")}</Link>
					)}
					{item.connectGithub && (
						<Link to="/find/github">{t("needsYouConnectGithub")}</Link>
					)}
					{hideable &&
						(snoozing ? (
							<>
								<Button
									onClick={() => {
										setSnoozing(false);
										onSnooze(item, "day");
									}}
								>
									{t("needsYouForADay")}
								</Button>
								<Button
									onClick={() => {
										setSnoozing(false);
										onSnooze(item, "week");
									}}
								>
									{t("needsYouForAWeek")}
								</Button>
							</>
						) : (
							<Button kind="quiet" onClick={() => setSnoozing(true)}>
								{t("needsYouSnooze")}
							</Button>
						))}
					{hideable && (
						<Button kind="quiet" onClick={() => onDismiss(item)}>
							{t("needsYouDismiss")}
						</Button>
					)}
				</div>
			)}
		</li>
	);
}
