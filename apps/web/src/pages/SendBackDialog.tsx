import { Button, Dialog, TextArea } from "@farik/ui";
import { useState } from "react";
import { t } from "../strings/t.ts";
import gate from "./Gate.module.css";
import type { Tries } from "./Gate.tsx";
import type { Criterion } from "./PlanPage.tsx";
import styles from "./pages.module.css";

/** Sending finished work back: what is not right, a required note, and which try this is. */
export function SendBackDialog({
	open,
	title,
	builder,
	criteria,
	tries,
	busy,
	onClose,
	onSend,
}: {
	open: boolean;
	title: string;
	builder: string;
	criteria: Criterion[];
	tries: Tries;
	busy: boolean;
	onClose: () => void;
	onSend: (failedCriteria: string[], message: string) => void;
}) {
	const [failed, setFailed] = useState<string[]>([]);
	const [note, setNote] = useState("");
	const flip = (id: string) =>
		setFailed((f) => (f.includes(id) ? f.filter((x) => x !== id) : [...f, id]));
	return (
		<Dialog
			open={open}
			title={t("sendBackTitle", { title, name: builder })}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("sendBackCancel")}</Button>
					<Button
						kind="primary"
						busy={busy}
						disabled={note.trim() === ""}
						onClick={() =>
							// Keep the criteria's order; "Something else" adds no id.
							onSend(
								criteria.map((c) => c.id).filter((id) => failed.includes(id)),
								note.trim(),
							)
						}
					>
						{t("sendBackSend", { name: builder })}
					</Button>
				</>
			}
		>
			<fieldset className={gate.criteria}>
				<legend>{t("sendBackWhat")}</legend>
				{criteria.map((c) => (
					<label key={c.id}>
						<input
							type="checkbox"
							checked={failed.includes(c.id)}
							onChange={() => flip(c.id)}
						/>
						{c.text}
					</label>
				))}
				<label>
					<input type="checkbox" />
					{t("sendBackElse")}
				</label>
			</fieldset>
			<TextArea
				id="send-back-note"
				label={t("sendBackNote", { name: builder })}
				value={note}
				onChange={setNote}
				required
			/>
			<p className={styles.muted}>
				{t("sendBackTry", { try: String(tries.try), of: String(tries.of) })}
			</p>
		</Dialog>
	);
}
