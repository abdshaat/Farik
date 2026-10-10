import { Button, Dialog, TextField } from "@catervas/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import { calendarDay, type OrderItem, todayIso } from "../orders.ts";
import styles from "../pages.module.css";
import { Ask } from "./Ask.tsx";
import { useSteps } from "./AskTeam.tsx";
import { DayField } from "./DayField.tsx";
import { visibly } from "./ToolApproval.tsx";

/**
 * "Mark PO-n received" (spec 6.10): the day it came, what the owner paid (what they said when
 * they placed it, else the total) and, for a subscription, the day it renews. Ticked, it also
 * files their request to update the register, in words they may change first.
 */
export function MarkReceived({
	order,
	agent,
	onDone,
	onClose,
}: {
	order: OrderItem;
	agent: string;
	/** Called when the order was marked received and, if ticked, the request was filed. */
	onDone: () => void;
	onClose: () => void;
}) {
	const [now] = useState(() => new Date());
	const [on, setOn] = useState(todayIso(now));
	const [paid, setPaid] = useState(order.paid ?? order.total);
	const [currency, setCurrency] = useState(
		order.paidCurrency ?? order.currency,
	);
	const [renews, setRenews] = useState("");
	const [ask, setAsk] = useState(true);
	const [edited, setEdited] = useState<string>();
	const { busy, refusal, send } = useSteps(() => {
		onDone();
		onClose();
	});
	const fill = {
		order: order.order,
		seller: visibly(order.seller),
		receivedOn: calendarDay(on || todayIso(now), now),
		paid: paid.trim(),
		currency: currency.trim(),
	};
	const draft =
		t(paid.trim() ? "receiveAskDraft" : "receiveAskDraftUnpaid", fill) +
		(renews
			? t("receiveAskRenews", { renewsOn: calendarDay(renews, now) })
			: "");
	const text = (edited ?? draft).trim();
	const mark = () =>
		send([
			{
				command: {
					command: "purchase_order_receive",
					body: {
						order: order.order,
						...(on ? { receivedOn: on } : {}),
						...(paid.trim()
							? { paid: paid.trim(), currency: currency.trim() }
							: {}),
						...(renews ? { renewsOn: renews } : {}),
					},
				},
			},
			...(ask ? [{ request: text }] : []),
		]);
	return (
		<Dialog
			open
			fillsPhone
			title={t("receiveTitle", {
				order: order.order,
				seller: visibly(order.seller),
			})}
			onClose={onClose}
			actions={
				<Button
					kind="primary"
					busy={busy}
					disabled={ask && text === ""}
					onClick={mark}
				>
					{t("ordersReceive")}
				</Button>
			}
		>
			<div className={styles.orderForm}>
				<DayField
					id="receive-on"
					label={t("receiveOn")}
					value={on}
					onChange={setOn}
				/>
				<div className={styles.money}>
					<TextField
						id="receive-paid"
						label={t("receivePaid")}
						value={paid}
						onChange={setPaid}
					/>
					<TextField
						id="receive-currency"
						label={t("placeCurrency")}
						value={currency}
						onChange={(value) => setCurrency(value.toUpperCase())}
					/>
				</div>
				<DayField
					id="receive-renews"
					label={t("receiveRenews")}
					hint={t("receiveRenewsHint")}
					value={renews}
					onChange={setRenews}
				/>
				<Ask
					id="receive-ask"
					label={t("receiveAsk", { name: agent })}
					checked={ask}
					onChecked={setAsk}
					text={edited ?? draft}
					onText={setEdited}
				/>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
