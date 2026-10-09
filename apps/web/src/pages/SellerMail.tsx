import { Avatar, type AvatarKey, Button } from "@farik/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { AskTeam } from "./dialogs/AskTeam.tsx";
import { SellerMessage, SendFields } from "./dialogs/SellerMessage.tsx";
import { FromLine, SellerReply } from "./dialogs/SellerReply.tsx";
import { useCommand } from "./dialogs/StartSprint.tsx";
import { visibly } from "./dialogs/ToolApproval.tsx";
import type {
	Mailbox,
	SellerMessageItem,
	SellerReplyItem,
} from "./sellerMail.ts";
import type { Agent } from "./setup/TeamSetup.tsx";
import styles from "./Today.module.css";

type Names = { agents: Agent[]; pm: string };

/** The agent a message or a reply belongs to, to show its picture and name. */
function who(agents: Agent[], id: string) {
	const agent = agents.find((a) => a.id === id);
	return { agent, name: agent?.displayName ?? id };
}

/**
 * "Messages to sellers" and "Replies from sellers" on Today (spec 6.10): what the Procurement
 * Specialist drafted for the owner to send, and what sellers answered. Nothing at all when both
 * are empty. An order's message is sent from its order, not here.
 */
export function SellerMailSections({ agents, pm }: Names) {
	const { data: mailbox } = useQuery<Mailbox>("procurement_mailbox.get", {});
	const { data: messages } = useQuery<{ messages: SellerMessageItem[] }>(
		"seller_messages.list",
		{},
	);
	const { data: replies } = useQuery<{ replies: SellerReplyItem[] }>(
		"seller_replies.list",
		{},
	);
	const unreadAny = (replies?.replies ?? []).some((one) => !one.dismissed);
	// The task titles a comparison names, asked only while a reply waits.
	const { data: tasks } = useQuery<{
		tasks: { taskId: string; title: string }[];
	}>("tasks.list", {}, !unreadAny);
	const waiting = (messages?.messages ?? []).filter(
		(one) => one.state === "waiting" && one.purchaseOrder === undefined,
	);
	const unread = (replies?.replies ?? []).filter((one) => !one.dismissed);
	return (
		<>
			{waiting.length > 0 && (
				<section className={styles.section} aria-labelledby="sellers-heading">
					<h2 id="sellers-heading">
						{t("sellerTitle", { n: waiting.length })}
					</h2>
					<p className={styles.muted}>{t("sellerLead")}</p>
					{!mailbox?.connected && (
						<p>
							{t("sellerNoMailbox")}{" "}
							<Link to="/team">{t("mailboxConnectLink")}</Link>
						</p>
					)}
					{mailbox?.connected && mailbox.sentToday >= mailbox.cap && (
						<p>{t("sellerCap")}</p>
					)}
					<ul
						className={styles.rows}
						aria-label={t("sellerTitle", { n: waiting.length })}
					>
						{waiting.map((one) => (
							<SellerMessageRow
								key={one.message}
								item={one}
								agents={agents}
								mailbox={mailbox}
							/>
						))}
					</ul>
				</section>
			)}
			{unread.length > 0 && (
				<section className={styles.section} aria-labelledby="replies-heading">
					<h2 id="replies-heading">{t("replyTitle", { n: unread.length })}</h2>
					<ul
						className={styles.rows}
						aria-label={t("replyTitle", { n: unread.length })}
					>
						{unread.map((one) => (
							<SellerReplyRow
								key={one.reply}
								item={one}
								agents={agents}
								pm={pm}
								messages={messages?.messages ?? []}
								titles={tasks?.tasks ?? []}
							/>
						))}
					</ul>
				</section>
			)}
		</>
	);
}

/** One message that waits to be sent: whole, with "Send", "Edit" and "Discard". */
function SellerMessageRow({
	item,
	agents,
	mailbox,
}: {
	item: SellerMessageItem;
	agents: Agent[];
	mailbox: Mailbox | undefined;
}) {
	const { agent, name } = who(agents, item.agentId);
	const seller = visibly(item.seller);
	const [editing, setEditing] = useState(false);
	const sent = useCommand(() => {});
	const discarded = useCommand(() => {});
	const capped = mailbox !== undefined && mailbox.sentToday >= mailbox.cap;
	const cannot = !mailbox?.connected || capped;
	const titleId = `seller-message-${item.message}`;
	const refusal = sent.refusal ?? discarded.refusal;
	return (
		<li className={styles.row}>
			{agent?.avatar && (
				<Avatar avatarKey={agent.avatar as AvatarKey} name={name} size={32} />
			)}
			<div className={styles.rowText}>
				<strong id={titleId}>
					{t(item.purpose === "question" ? "sellerQuestion" : "sellerQuote", {
						name,
						seller,
					})}
				</strong>
				<SendFields
					seller={item.seller}
					to={item.to}
					domain={item.domain}
					newDomain={item.newDomain}
					mailbox={mailbox}
				/>
				<span>
					<strong>{t("sellerSubject")}</strong> {visibly(item.subject)}
				</span>
				<span className={styles.muted} id={`${titleId}-body`}>
					{t("sellerBody", { name })}
				</span>
				{/* The agent's own words: React renders them as text, never as markup. */}
				<fieldset
					aria-labelledby={`${titleId}-body`}
					data-trust="untrusted"
					className={styles.frame}
				>
					{visibly(item.body)}
				</fieldset>
				{item.why && (
					<p role="alert">{t("sellerFailed", { why: visibly(item.why) })}</p>
				)}
				{refusal && <p role="alert">{refusal}</p>}
			</div>
			{/* The group names the message, so that each row's "Send" is told from the others. */}
			<fieldset className={styles.decide} aria-labelledby={titleId}>
				<Button
					kind="primary"
					busy={sent.busy}
					disabled={cannot}
					onClick={() =>
						sent.send({
							command: "seller_message_send",
							body: {
								message: item.message,
								subject: item.subject,
								body: item.body,
							},
						})
					}
				>
					{t("sellerSend")}
				</Button>
				<Button onClick={() => setEditing(true)}>{t("sellerEdit")}</Button>
				<Button
					busy={discarded.busy}
					onClick={() =>
						discarded.send({
							command: "seller_message_discard",
							body: { message: item.message },
						})
					}
				>
					{t("sellerDiscard")}
				</Button>
			</fieldset>
			{editing && (
				<SellerMessage
					item={item}
					agent={name}
					mailbox={mailbox}
					capped={capped}
					onClose={() => setEditing(false)}
				/>
			)}
		</li>
	);
}

/** One reply from a seller: "Read", "Ask for a comparison" or a follow-up, and "Dismiss". */
function SellerReplyRow({
	item,
	agents,
	pm,
	messages,
	titles,
}: {
	item: SellerReplyItem;
	agents: Agent[];
	pm: string;
	messages: SellerMessageItem[];
	titles: { taskId: string; title: string }[];
}) {
	const message = messages.find((one) => one.message === item.message);
	const { agent, name } = who(agents, message?.agentId ?? "");
	const seller = visibly(item.seller);
	const [dialog, setDialog] = useState<"read" | "ask">();
	const dismissed = useCommand(() => setDialog(undefined));
	const taskId = message?.taskId ?? "";
	const title = titles.find((one) => one.taskId === taskId)?.title ?? "";
	const kept = item.attachments.filter((file) => file.kept).length;
	const titleId = `seller-reply-${item.reply}`;
	const dismiss = {
		command: "seller_reply_dismiss",
		body: { reply: item.reply },
	};
	const ask = item.order === undefined ? "replyCompare" : "ordersFollowUp";
	const buttons = (
		<>
			<Button onClick={() => setDialog("ask")}>{t(ask)}</Button>
			<Button busy={dismissed.busy} onClick={() => dismissed.send(dismiss)}>
				{t("replyDismiss")}
			</Button>
		</>
	);
	return (
		<li className={styles.row}>
			{agent?.avatar && (
				<Avatar avatarKey={agent.avatar as AvatarKey} name={name} size={32} />
			)}
			<div className={styles.rowText}>
				<strong id={titleId}>
					{t("replyLine", { seller, subject: visibly(item.sentSubject) })}
				</strong>
				<FromLine address={item.from} seller={item.seller} />
				{kept > 0 && (
					<span>
						{kept === 1 ? t("replyKeptOne") : t("replyKept", { n: kept })}
					</span>
				)}
				{dismissed.refusal && <p role="alert">{dismissed.refusal}</p>}
			</div>
			<fieldset className={styles.decide} aria-labelledby={titleId}>
				<Button kind="primary" onClick={() => setDialog("read")}>
					{t("replyRead")}
				</Button>
				{buttons}
			</fieldset>
			{dialog === "read" && (
				<SellerReply
					reply={item.reply}
					seller={item.seller}
					agent={name}
					sentSubject={item.sentSubject}
					from={item.from}
					subject={item.subject}
					text={item.text}
					receivedAt={item.receivedAt}
					files={item.attachments}
					actions={buttons}
					onClose={() => setDialog(undefined)}
				/>
			)}
			{dialog === "ask" && (
				<AskTeam
					title={
						item.order === undefined
							? t("replyCompareTitle", { name })
							: t("followUpTitle", { name, order: item.order })
					}
					draft={
						item.order === undefined
							? t("replyCompareDraft", { task: taskId, title })
							: t("followUpDraft", { order: item.order, seller })
					}
					pm={pm}
					then={dismiss}
					onClose={() => setDialog(undefined)}
				/>
			)}
		</li>
	);
}
