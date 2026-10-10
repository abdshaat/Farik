import { Avatar, type AvatarKey, Button } from "@catervas/ui";
import { useId, useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { PurchaseOrder } from "./dialogs/PurchaseOrder.tsx";
import { isScript, ScriptWarning } from "./dialogs/SiteRequest.tsx";
import { visibly } from "./dialogs/ToolApproval.tsx";
import {
	aheadDay,
	amount,
	figure,
	type OrderAsk,
	periodWords,
} from "./orders.ts";
import type { Mailbox } from "./sellerMail.ts";
import type { Agent } from "./setup/TeamSetup.tsx";
import styles from "./Today.module.css";

/** An order's row of `waiting.list`: the order, and the task it came from. */
export type OrderWaiting = OrderAsk & {
	kind: "purchase_order";
	taskId: string;
	title: string;
	agentId: string | null;
};

/** How many lines of an order its row shows before it says how many more there are. */
const SHOWN = 5;

/** The workbook's media type, which the browser is told when it is given the file. */
const WORKBOOK =
	"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/**
 * "Download PO-n.xlsx": the workbook Catervas wrote for the order, fetched from the daemon and handed
 * to the browser to keep (spec 6.10). It says so when the file is gone.
 */
export function DownloadOrder({ order }: { order: number }) {
	const { client } = useConnection();
	const [busy, setBusy] = useState(false);
	const [gone, setGone] = useState(false);
	const download = async () => {
		if (!client) return;
		setBusy(true);
		setGone(false);
		try {
			const file = (await client.call("purchase_order.file", { order })) as {
				mediaType?: string;
				base64: string;
			};
			const bytes = Uint8Array.from(atob(file.base64), (c) => c.charCodeAt(0));
			const url = URL.createObjectURL(
				new Blob([bytes], { type: file.mediaType ?? WORKBOOK }),
			);
			const link = document.createElement("a");
			link.href = url;
			link.download = `PO-${order}.xlsx`;
			document.body.append(link);
			link.click();
			link.remove();
			setTimeout(() => URL.revokeObjectURL(url), 0);
		} catch {
			setGone(true);
		} finally {
			setBusy(false);
		}
	};
	return (
		<span className={styles.download}>
			<button
				type="button"
				className={styles.downloadLink}
				disabled={busy}
				onClick={download}
			>
				{t("orderDownload", { order })}
			</button>
			{gone && <span role="alert">{t("orderFileGone")}</span>}
		</span>
	);
}

/** "Read the comparison": the note the order rests on, fetched when it is opened, as text. */
function Comparison({ order }: { order: number }) {
	const [open, setOpen] = useState(false);
	const summary = useId();
	const { data, error } = useQuery<{ text: string }>(
		"purchase_order.evaluation",
		{ order },
		!open,
	);
	return (
		<details
			className={styles.comparison}
			onToggle={(e) => setOpen(e.currentTarget.open)}
		>
			<summary id={summary}>{t("orderComparison")}</summary>
			{open && data && (
				// The agent's own words: React renders them as text, never as markup.
				<fieldset
					aria-labelledby={summary}
					data-trust="untrusted"
					className={`${styles.frame} ${styles.comparisonText}`}
					// biome-ignore lint/a11y/noNoninteractiveTabindex: a scroll box must take focus to scroll by keyboard
					tabIndex={0}
				>
					{visibly(data.text)}
				</fieldset>
			)}
			{open && !data && error && <p>{t("orderComparisonGone")}</p>}
		</details>
	);
}

/** The address as a link that opens in a tab that cannot reach this page, when it is a plain `https` one. */
function opened(url: string): string | undefined {
	try {
		const parsed = new URL(url);
		return parsed.protocol === "https:" ? parsed.href : undefined;
	} catch {
		return undefined;
	}
}

/**
 * A purchase order waiting for the owner: what it holds line by line, its total, the seller's page
 * on a site the owner allowed, why, the comparison it rests on and the file Catervas wrote, and the
 * owner's "Approve, I'll place it myself" or "Reject", each of which asks for a note (spec 6.10).
 * Everything the agent wrote is shown as text.
 */
export function PurchaseOrderRow({
	item,
	agent,
	now,
}: {
	item: OrderWaiting;
	agent: Agent | undefined;
	now: Date;
}) {
	const name = agent?.displayName ?? item.agentId ?? "";
	const titleId = `waiting-order-${item.order}`;
	const whyId = `${titleId}-why`;
	const [dialog, setDialog] = useState<"approve" | "reject" | "send">();
	const { data: mailbox } = useQuery<Mailbox>("procurement_mailbox.get", {});
	// With the order's email drafted and a mailbox connected, the owner may send it from here.
	const sendable =
		item.send !== undefined &&
		mailbox?.connected === true &&
		mailbox.sentToday < mailbox.cap;
	const seller = visibly(item.seller);
	const host = item.host ?? "";
	const address = item.url === "" ? undefined : item.url;
	const link = address && opened(address);
	const [forTask, forTaskEnd] = t("orderTask", { task: "{task}" }).split(
		"{task}",
	);
	const more = item.lines.length - SHOWN;
	const about: [string, string][] = [
		[t("orderContact"), item.sellerContact],
		[t("orderDelivery"), item.delivery],
		[t("orderTerms"), item.terms],
	];
	return (
		<li className={styles.row}>
			{agent?.avatar && (
				<Avatar avatarKey={agent.avatar as AvatarKey} name={name} size={32} />
			)}
			<div className={styles.rowText}>
				<strong id={titleId}>{t("orderLine", { name, seller })}</strong>
				<span>
					{forTask}
					<Link to={`/tasks/${item.taskId}`}>
						{item.taskId} {item.title}
					</Link>
					{forTaskEnd}
				</span>
				<table className={styles.lines}>
					<thead>
						<tr>
							<th scope="col">{t("orderItem")}</th>
							<th scope="col">{t("orderQuantity")}</th>
							<th scope="col">{t("orderPrice")}</th>
							<th scope="col">{t("orderLineTotal")}</th>
						</tr>
					</thead>
					<tbody>
						{item.lines.slice(0, SHOWN).map((line, index) => (
							// The agent may write the same item twice: the line's place is its key.
							// biome-ignore lint/suspicious/noArrayIndexKey: the lines are never reordered
							<tr key={index}>
								<td data-label={t("orderItem")}>{visibly(line.item)}</td>
								<td data-label={t("orderQuantity")}>
									{[line.quantity, visibly(line.unit)]
										.filter((part) => part !== "")
										.join(" ")}
								</td>
								<td data-label={t("orderPrice")}>
									{amount(line.unitPrice, item.currency)}
								</td>
								<td data-label={t("orderLineTotal")}>
									{amount(line.lineTotal, item.currency)}
								</td>
							</tr>
						))}
					</tbody>
				</table>
				{more > 0 && (
					<span className={styles.muted}>
						{more === 1
							? t("orderMoreLinesOne")
							: t("orderMoreLines", { n: more })}
					</span>
				)}
				<p className={styles.total}>
					{t("orderTotalLabel")}{" "}
					<strong className={styles.amount}>
						{figure(item.total)} {item.currency}
					</strong>{" "}
					{periodWords(item.period)}
				</p>
				<dl className={styles.about}>
					{about
						.filter(([, text]) => text !== "")
						.map(([label, text]) => (
							<div key={label}>
								<dt>{label}</dt>
								<dd>{visibly(text)}</dd>
							</div>
						))}
				</dl>
				<span className={styles.muted}>{t("orderAsWritten", { name })}</span>
				<span className={styles.muted}>{t("orderPage")}</span>
				{address ? (
					<>
						{host && (
							<strong>
								<code className={styles.host}>{host}</code>
							</strong>
						)}
						{isScript(host) && <ScriptWarning className={styles.script} />}
						<code className={styles.address}>{visibly(address)}</code>
						<span>
							{link && (
								<a href={link} target="_blank" rel="noopener noreferrer">
									{t("orderOpen")}
								</a>
							)}{" "}
							<span className={styles.muted}>
								{t("orderCheck", { seller })}
							</span>
						</span>
					</>
				) : (
					<span>{t("orderNoPage")}</span>
				)}
				<span className={styles.muted} id={whyId}>
					{t("orderWhy", { name })}
				</span>
				{/* The agent's own words: React renders them as text, never as markup. */}
				<fieldset
					aria-labelledby={whyId}
					data-trust="untrusted"
					className={styles.frame}
				>
					{visibly(item.why)}
				</fieldset>
				<Comparison order={item.order} />
				<DownloadOrder order={item.order} />
				<span className={styles.muted}>
					{t("orderCloses", { day: aheadDay(item.expiresAt, now) })}
				</span>
			</div>
			{/* The group names the order, so that each row's "Approve" is told from the others. */}
			<fieldset className={styles.decide} aria-labelledby={titleId}>
				{sendable && (
					<Button kind="primary" onClick={() => setDialog("send")}>
						{t("orderApproveSend", { seller })}
					</Button>
				)}
				<Button
					kind={sendable ? "secondary" : "primary"}
					onClick={() => setDialog("approve")}
				>
					{t("orderApprove")}
				</Button>
				<Button onClick={() => setDialog("reject")}>{t("orderReject")}</Button>
			</fieldset>
			{dialog && (
				<PurchaseOrder
					order={item.order}
					seller={item.seller}
					agent={name}
					approve={dialog !== "reject"}
					{...(dialog === "send" && item.send
						? { send: item.send, mailbox }
						: {})}
					onClose={() => setDialog(undefined)}
				/>
			)}
		</li>
	);
}
