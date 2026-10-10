import { Button, Dialog, TextArea } from "@catervas/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { useCommand } from "./StartSprint.tsx";

/**
 * "Close PO-n without receiving it" (spec 6.10): the seller cancelled or refunded the order, or it
 * was lost. The agent stops following it up, and reads the owner's note in its next piece of work.
 */
export function CloseOrder({
	order,
	agent,
	onDone,
	onClose,
}: {
	order: number;
	agent: string;
	onDone: () => void;
	onClose: () => void;
}) {
	const [note, setNote] = useState("");
	const { busy, refusal, send } = useCommand(() => {
		onDone();
		onClose();
	});
	return (
		<Dialog
			open
			fillsPhone
			title={t("closeTitle", { order })}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("closeKeep")}</Button>
					<Button
						kind="primary"
						busy={busy}
						onClick={() =>
							send({
								command: "purchase_order_close",
								body: {
									order,
									...(note.trim() ? { note: note.trim() } : {}),
								},
							})
						}
					>
						{t("closeDo")}
					</Button>
				</>
			}
		>
			<div className={styles.orderForm}>
				<p>{t("closeBody", { name: agent })}</p>
				<TextArea
					id="close-order-note"
					label={t("orderNote", { name: agent })}
					value={note}
					onChange={setNote}
				/>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
