import { Dialog } from "@farik/ui";
import { type ReactNode, useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { type ReplyFile, sizeWords } from "../sellerMail.ts";
import { visibly } from "./ToolApproval.tsx";

/** The reply's From line, as text, with the warning that anyone can write any address. */
export function FromLine({
	address,
	seller,
	className,
}: {
	address: string;
	seller: string;
	className?: string | undefined;
}) {
	return (
		<>
			<span>
				{t("replyFrom", { address: "" })}
				<code className={className}>{visibly(address)}</code>
			</span>
			<span className={className}>
				{t("replyCheck", { seller: visibly(seller) })}
			</span>
		</>
	);
}

/** "Download": a kept attachment, fetched from the daemon and handed to the browser to keep. */
function Download({ reply, file }: { reply: number; file: ReplyFile }) {
	const { client } = useConnection();
	const [busy, setBusy] = useState(false);
	const [gone, setGone] = useState(false);
	const download = async () => {
		if (!client) return;
		setBusy(true);
		setGone(false);
		try {
			const got = (await client.call("seller_reply.attachment", {
				reply,
				index: file.index,
			})) as { mediaType: string; base64: string; name: string };
			const bytes = Uint8Array.from(atob(got.base64), (c) => c.charCodeAt(0));
			const url = URL.createObjectURL(
				new Blob([bytes], { type: got.mediaType }),
			);
			const link = document.createElement("a");
			link.href = url;
			link.download = got.name;
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
		<>
			<button type="button" disabled={busy} onClick={download}>
				{t("replyDownload")}
				<span hidden> {visibly(file.name)}</span>
			</button>
			{gone && <span role="alert">{t("replyGone")}</span>}
		</>
	);
}

/**
 * A reply from a seller, read: everything they wrote as text in one frame that says whose words
 * they are (no address or link in it is a link), the attachments Farik kept, and the owner's two
 * steps (spec 6.10, 8.6). `actions` are the row's buttons, which close it.
 */
export function SellerReply({
	reply,
	seller,
	agent,
	sentSubject,
	from,
	subject,
	text,
	receivedAt,
	files,
	actions,
	onClose,
}: {
	reply: number;
	seller: string;
	agent: string;
	sentSubject: string;
	from: string;
	subject: string;
	text: string;
	receivedAt: string;
	files: ReplyFile[];
	actions: ReactNode;
	onClose: () => void;
}) {
	const received = new Date(receivedAt);
	const day = new Intl.DateTimeFormat("en-GB", {
		day: "numeric",
		month: "long",
	}).format(received);
	const time = new Intl.DateTimeFormat("en-GB", {
		hour: "2-digit",
		minute: "2-digit",
	}).format(received);
	return (
		<Dialog
			open
			fillsPhone
			title={t("replyDialogTitle", { seller: visibly(seller) })}
			onClose={onClose}
			actions={actions}
		>
			<div className={styles.orderForm}>
				<FromLine address={from} seller={seller} />
				<p>{t("replyReceived", { day, time })}</p>
				<p>{t("replyTo", { subject: visibly(sentSubject) })}</p>
				<span id="reply-text">{t("replyText")}</span>
				{/* The seller's own words: React renders them as text, never as markup or links. */}
				<fieldset
					aria-labelledby="reply-text"
					data-trust="untrusted"
					className={styles.orderForm}
				>
					<strong>{visibly(subject)}</strong>
					<pre>{visibly(text)}</pre>
				</fieldset>
				{files.length > 0 && (
					<>
						<span id="reply-files">{t("replyAttachments")}</span>
						<ul aria-labelledby="reply-files">
							{files.map((file) => (
								<li key={file.index}>
									{file.kept ? (
										<>
											{t("replyFile", {
												kind:
													file.mediaType === "application/pdf"
														? t("replyPdf")
														: t("replyPicture"),
												size: sizeWords(file.bytes),
												name: visibly(file.name),
											})}{" "}
											<Download reply={reply} file={file} />
										</>
									) : (
										t("replySkipped", { name: visibly(file.name) })
									)}
								</li>
							))}
						</ul>
					</>
				)}
				<p className={styles.orderHint}>
					{t("replyUntrusted", { name: agent, seller: visibly(seller) })}
				</p>
			</div>
		</Dialog>
	);
}
