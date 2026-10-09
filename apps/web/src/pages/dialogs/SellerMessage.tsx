import { Button, Dialog, TextArea, TextField } from "@farik/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import type { Mailbox } from "../sellerMail.ts";
import { adds as addsOf } from "../sellerMail.ts";
import { isScript, ScriptWarning } from "./SiteRequest.tsx";
import { useCommand } from "./StartSprint.tsx";
import { visibly } from "./ToolApproval.tsx";

/**
 * The address, its domain set apart in bold and the code face, in its ASCII form: the agent's own
 * spelling of what comes before the domain is kept, and the domain is the one Farik sends to.
 */
export function ToAddress({
	to,
	domain,
}: {
	to: string;
	domain: string | null;
}) {
	const at = to.lastIndexOf("@");
	const host = domain ?? to.slice(at + 1);
	return (
		<>
			<code>
				{visibly(to.slice(0, at + 1))}
				<strong>{visibly(host)}</strong>
			</code>
			{isScript(host) && <ScriptWarning className={styles.orderHint} />}
		</>
	);
}

/**
 * What a message goes out with that the owner did not read in the agent's draft: From, To (with
 * the warning for a domain nothing was sent to before), and the lines Farik adds (spec 6.10).
 */
export function SendFields({
	seller,
	to,
	domain,
	newDomain,
	mailbox,
}: {
	seller: string;
	to: string;
	domain: string | null;
	newDomain: boolean;
	mailbox: Mailbox | undefined;
}) {
	const added = addsOf(
		mailbox,
		t("mailboxDisclosure", { name: mailbox?.name ?? "" }),
	);
	return (
		<>
			<p>
				<strong>{t("sellerFromLabel")}</strong>{" "}
				{mailbox?.connected && (
					<>
						{visibly(mailbox.name ?? "")} <code>{mailbox.address}</code>
					</>
				)}
			</p>
			<div>
				<strong>{t("sellerToLabel")}</strong>{" "}
				<ToAddress to={to} domain={domain} />
			</div>
			{newDomain && (
				<p role="note">
					{t("sellerNewDomain", {
						domain: visibly(domain ?? ""),
						seller: visibly(seller),
					})}
				</p>
			)}
			{added !== "" && (
				<>
					<span className={styles.orderHint}>{t("sellerAdds")}</span>
					<pre>{visibly(added)}</pre>
				</>
			)}
		</>
	);
}

/**
 * "Edit" a message to a seller: the subject and the words are the owner's to change, From and To
 * stay as the agent wrote them, and Send sends what is shown (spec 6.10). Close sends nothing.
 */
export function SellerMessage({
	item,
	agent,
	mailbox,
	capped,
	onClose,
}: {
	item: {
		message: number;
		seller: string;
		to: string;
		domain: string | null;
		newDomain: boolean;
		subject: string;
		body: string;
		why?: string | undefined;
	};
	agent: string;
	mailbox: Mailbox | undefined;
	/** Whether the day's messages are used up. */
	capped: boolean;
	onClose: () => void;
}) {
	const [subject, setSubject] = useState(item.subject);
	const [body, setBody] = useState(item.body);
	const { busy, refusal, send } = useCommand(onClose);
	const cannot = !mailbox?.connected || capped;
	return (
		<Dialog
			open
			fillsPhone
			title={t("sellerEditTitle", { seller: visibly(item.seller) })}
			onClose={onClose}
			actions={
				!cannot && (
					<Button
						kind="primary"
						busy={busy}
						disabled={subject.trim() === "" || body.trim() === ""}
						onClick={() =>
							send({
								command: "seller_message_send",
								body: { message: item.message, subject: subject.trim(), body },
							})
						}
					>
						{t("sellerSend")}
					</Button>
				)
			}
		>
			<div className={styles.orderForm}>
				<SendFields
					seller={item.seller}
					to={item.to}
					domain={item.domain}
					newDomain={item.newDomain}
					mailbox={mailbox}
				/>
				<TextField
					id="seller-subject"
					label={t("sellerSubject")}
					value={subject}
					onChange={setSubject}
				/>
				<TextArea
					id="seller-message"
					label={t("sellerMessageField")}
					value={body}
					onChange={setBody}
					rows={8}
				/>
				<p className={styles.orderHint}>
					{t("sellerEditBody", { name: agent })}
				</p>
				{item.why && (
					<p role="alert">{t("sellerFailed", { why: visibly(item.why) })}</p>
				)}
				{!mailbox?.connected && <p>{t("sellerNoMailbox")}</p>}
				{capped && <p>{t("sellerCap")}</p>}
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
