import { Button, Dialog, TextField } from "@farik/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import { calendarDay, type OrderItem, todayIso } from "../orders.ts";
import styles from "../pages.module.css";
import { Ask } from "./Ask.tsx";
import { useSteps } from "./AskTeam.tsx";
import { DayField } from "./DayField.tsx";
import { visibly } from "./ToolApproval.tsx";

/**
 * "Mark PO-n placed" (spec 6.10): the day the owner placed it and, if they know, what they paid.
 * Ticked, it also files their request to follow the order up until it arrives, in words they may
 * change first. Farik places nothing: this records what the owner did.
 */
export function MarkPlaced({
	order,
	agent,
	onDone,
	onClose,
}: {
	order: OrderItem;
	agent: string;
	/** Called when the order was marked placed and, if ticked, the request was filed. */
	onDone: () => void;
	onClose: () => void;
}) {
	const [now] = useState(() => new Date());
	const [on, setOn] = useState(todayIso(now));
	const [paid, setPaid] = useState("");
	const [currency, setCurrency] = useState(order.currency);
	const [ask, setAsk] = useState(true);
	const [edited, setEdited] = useState<string>();
	const { busy, refusal, send } = useSteps(() => {
		onDone();
		onClose();
	});
	const draft = t("placeAskDraft", {
		order: order.order,
		seller: visibly(order.seller),
		placedOn: calendarDay(on || todayIso(now), now),
	});
	const text = (edited ?? draft).trim();
	const mark = () =>
		send([
			{
				command: {
					command: "purchase_order_place",
					body: {
						order: order.order,
						...(on ? { placedOn: on } : {}),
						...(paid.trim()
							? { paid: paid.trim(), currency: currency.trim() }
							: {}),
					},
				},
			},
			...(ask ? [{ request: text }] : []),
		]);
	return (
		<Dialog
			open
			fillsPhone
			title={t("placeTitle", {
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
					{t("ordersPlace")}
				</Button>
			}
		>
			<div className={styles.orderForm}>
				<DayField
					id="place-on"
					label={t("placeOn")}
					value={on}
					onChange={setOn}
				/>
				<div className={styles.money}>
					<TextField
						id="place-paid"
						label={t("placePaid")}
						value={paid}
						onChange={setPaid}
					/>
					<TextField
						id="place-currency"
						label={t("placeCurrency")}
						value={currency}
						onChange={(value) => setCurrency(value.toUpperCase())}
					/>
				</div>
				<p className={styles.orderHint}>{t("placePaidHint")}</p>
				<Ask
					id="place-ask"
					label={t("placeAsk", { name: agent })}
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
