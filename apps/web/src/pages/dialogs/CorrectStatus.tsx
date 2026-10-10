import { Button, Choice, Dialog, TextField, uiStrings } from "@catervas/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import { type OrderItem, STATUSES, statusWords, todayIso } from "../orders.ts";
import styles from "../pages.module.css";
import { DayField } from "./DayField.tsx";
import { useCommand } from "./StartSprint.tsx";

/** The most characters a status's note has (spec 6.10); the daemon refuses a longer one. */
const MOST_NOTE = 300;

/**
 * "Correct the status of PO-n" (spec 6.10): the owner says where a placed order really stands,
 * when they know better than the agent's last follow-up. Delayed needs what they know and the day
 * it is expected; a problem needs what they know. The note is at most 300 characters and the day
 * today or later, which the dialog says at the field instead of sending what the daemon would
 * refuse under a sentence about Delayed.
 */
export function CorrectStatus({
	order,
	agent,
	onDone,
	onClose,
}: {
	order: OrderItem;
	agent: string;
	onDone: () => void;
	onClose: () => void;
}) {
	const now = order.status?.status;
	const [status, setStatus] = useState(
		STATUSES.some(([name]) => name === now) ? (now as string) : "preparing",
	);
	const [today] = useState(() => todayIso(new Date()));
	const [note, setNote] = useState("");
	const [expected, setExpected] = useState("");
	const { busy, refusal, send } = useCommand(() => {
		onDone();
		onClose();
	});
	const needsNote = status === "delayed" || status === "problem";
	const needsDay = status === "delayed";
	const noteTooLong = note.trim().length > MOST_NOTE;
	const dayPassed = expected !== "" && expected < today;
	const required = ` (${uiStrings.required})`;
	const save = () =>
		send({
			command: "purchase_order_update",
			body: {
				order: order.order,
				status,
				...(note.trim() ? { note: note.trim() } : {}),
				...(expected ? { expectedOn: expected } : {}),
			},
		});
	return (
		<Dialog
			open
			fillsPhone
			title={t("correctTitle", { order: order.order })}
			onClose={onClose}
			actions={
				<Button
					kind="primary"
					busy={busy}
					disabled={
						(needsNote && !note.trim()) ||
						(needsDay && !expected) ||
						noteTooLong ||
						dayPassed
					}
					onClick={save}
				>
					{t("correctSave")}
				</Button>
			}
		>
			<div className={styles.orderForm}>
				<Choice
					name="correct-status"
					legend={t("correctLegend")}
					options={STATUSES.map(([value]) => ({
						value,
						label: statusWords(value),
					}))}
					value={status}
					onChange={setStatus}
				/>
				<TextField
					id="correct-note"
					label={t("correctNote") + (needsNote ? required : "")}
					value={note}
					onChange={setNote}
					{...(noteTooLong && { error: t("correctNoteLong") })}
				/>
				<DayField
					id="correct-expected"
					label={t("correctExpected") + (needsDay ? required : "")}
					value={expected}
					onChange={setExpected}
					min={today}
					error={dayPassed ? t("correctDayPast") : undefined}
				/>
				<p className={styles.orderHint}>{t("correctBody", { name: agent })}</p>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
