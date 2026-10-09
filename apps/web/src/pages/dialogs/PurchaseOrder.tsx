import { Button, Dialog, TextArea, TextField } from "@farik/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import { calendarDay, todayIso } from "../orders.ts";
import styles from "../pages.module.css";
import type { Mailbox, OrderSend } from "../sellerMail.ts";
import { Ask } from "./Ask.tsx";
import { useSteps } from "./AskTeam.tsx";
import { SendFields } from "./SellerMessage.tsx";
import { useCommand } from "./StartSprint.tsx";
import { visibly } from "./ToolApproval.tsx";

/**
 * "Approve PO-n" or "Reject PO-n", with a note the agent reads in its next piece of work (spec
 * 6.10). Approving places nothing: the owner places the order and pays for it themselves. With
 * `send`, approving also emails the order to the seller from the procurement mailbox, with the
 * workbook attached: the owner's press is the only thing that sends it.
 */
export function PurchaseOrder({
	order,
	seller,
	agent,
	approve,
	send,
	mailbox,
	onClose,
}: {
	order: number;
	/** The seller, in the agent's words. */
	seller: string;
	agent: string;
	approve: boolean;
	/** The order's email, when the owner sends it with their approval. */
	send?: OrderSend;
	mailbox?: Mailbox | undefined;
	onClose: () => void;
}) {
	const [note, setNote] = useState("");
	const { busy, refusal, send: sendCommand } = useCommand(onClose);
	const decide = () => {
		const said = note.trim();
		sendCommand({
			command: "purchase_order_decide",
			body: {
				order,
				decision: approve ? "approve" : "reject",
				...(said ? { note: said } : {}),
			},
		});
	};
	if (send) {
		return (
			<SendForm
				order={order}
				seller={seller}
				agent={agent}
				send={send}
				mailbox={mailbox}
				note={note}
				onNote={setNote}
				onClose={onClose}
			/>
		);
	}
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

/**
 * "Approve PO-n and send it to <seller>": the email is shown whole and may be edited, the owner
 * may ask for a follow-up, and "Approve and send" sends the order, then files the request. A
 * refused or failed send files nothing.
 */
function SendForm({
	order,
	seller,
	agent,
	send,
	mailbox,
	note,
	onNote,
	onClose,
}: {
	order: number;
	seller: string;
	agent: string;
	send: OrderSend;
	mailbox: Mailbox | undefined;
	note: string;
	onNote: (note: string) => void;
	onClose: () => void;
}) {
	const [now] = useState(() => new Date());
	const [subject, setSubject] = useState(send.subject);
	const [body, setBody] = useState(send.body);
	const [ask, setAsk] = useState(true);
	const [edited, setEdited] = useState<string>();
	const { busy, refusal, send: steps } = useSteps(onClose);
	const draft = t("placeAskDraft", {
		order,
		seller: visibly(seller),
		placedOn: calendarDay(todayIso(now), now),
	});
	const text = (edited ?? draft).trim();
	const said = note.trim();
	return (
		<Dialog
			open
			fillsPhone
			title={t("orderSendTitle", { order, seller: visibly(seller) })}
			onClose={onClose}
			actions={
				<Button
					kind="primary"
					busy={busy}
					disabled={
						subject.trim() === "" || body.trim() === "" || (ask && text === "")
					}
					onClick={() =>
						steps([
							{
								command: {
									command: "purchase_order_send",
									body: {
										order,
										message: send.message,
										subject: subject.trim(),
										body,
										...(said ? { note: said } : {}),
									},
								},
							},
							...(ask ? [{ request: text }] : []),
						])
					}
				>
					{t("orderSendButton")}
				</Button>
			}
		>
			<div className={styles.orderForm}>
				<p>{t("orderSendBody", { seller: visibly(seller) })}</p>
				<SendFields
					seller={seller}
					to={send.to}
					domain={send.domain}
					newDomain={send.newDomain}
					mailbox={mailbox}
				/>
				<TextField
					id="order-send-subject"
					label={t("sellerSubject")}
					value={subject}
					onChange={setSubject}
				/>
				<TextArea
					id="order-send-message"
					label={t("sellerMessageField")}
					value={body}
					onChange={setBody}
					rows={8}
				/>
				<p className={styles.orderHint}>{t("orderAttached", { order })}</p>
				<Ask
					id="order-send-ask"
					label={t("placeAsk", { name: agent })}
					checked={ask}
					onChecked={setAsk}
					text={edited ?? draft}
					onText={setEdited}
				/>
				<TextArea
					id="purchase-order-note"
					label={t("orderNote", { name: agent })}
					value={note}
					onChange={onNote}
				/>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
