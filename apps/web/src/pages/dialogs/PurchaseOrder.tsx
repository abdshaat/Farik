import { Button, Dialog, TextArea } from "@farik/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { useCommand } from "./StartSprint.tsx";
import { visibly } from "./ToolApproval.tsx";

/**
 * "Approve PO-n" or "Reject PO-n", with a note the agent reads in its next piece of work (spec
 * 6.10). Approving places nothing: the owner places the order and pays for it themselves.
 */
export function PurchaseOrder({
	order,
	seller,
	agent,
	approve,
	onClose,
}: {
	order: number;
	/** The seller, in the agent's words. */
	seller: string;
	agent: string;
	approve: boolean;
	onClose: () => void;
}) {
	const [note, setNote] = useState("");
	const { busy, refusal, send } = useCommand(onClose);
	const decide = () => {
		const said = note.trim();
		send({
			command: "purchase_order_decide",
			body: {
				order,
				decision: approve ? "approve" : "reject",
				...(said ? { note: said } : {}),
			},
		});
	};
	return (
		<Dialog
			open
			fillsPhone
			title={t(approve ? "orderApproveTitle" : "orderRejectTitle", {
				order,
				seller: visibly(seller),
			})}
			onClose={onClose}
			actions={
				<Button kind="primary" busy={busy} onClick={decide}>
					{t(approve ? "orderApproveDo" : "orderReject")}
				</Button>
			}
		>
			<div className={styles.orderForm}>
				<p>
					{t(approve ? "orderApproveBody" : "orderRejectBody", { name: agent })}
				</p>
				<TextArea
					id="purchase-order-note"
					label={t("orderNote", { name: agent })}
					value={note}
					onChange={setNote}
				/>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
