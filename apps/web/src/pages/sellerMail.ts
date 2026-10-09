// The Procurement Specialist's mailbox, its messages to sellers and their replies as the daemon
// gives them (`procurement_mailbox.get`, `seller_messages.list`, `seller_replies.list`, and the
// `send` of a `purchase_order` row of `waiting.list`), and the words and times they are said with
// (spec 6.10, ADR 0039).

/** The connected mailbox, or `connected: false`. */
export type Mailbox = {
	connected: boolean;
	address?: string;
	name?: string;
	provider?: string;
	folder?: string;
	signature?: string;
	discloseAi?: boolean;
	checkedAt?: string;
	/** Farik's sentence about why the last check failed. */
	error?: string;
	restartedAt?: string;
	sentToday: number;
	cap: number;
};

/** A message to a seller. Everything the agent wrote in it is untrusted. */
export type SellerMessageItem = {
	message: number;
	state: string;
	seller: string;
	to: string;
	/** The address's domain in its ASCII form. */
	domain: string | null;
	newDomain: boolean;
	subject: string;
	body: string;
	purpose: string;
	taskId: string;
	agentId: string;
	draftedAt: string;
	purchaseOrder?: number;
	/** Farik's sentence about the last try that failed. */
	why?: string;
};

/** An order's message as its `waiting.list` row carries it. */
export type OrderSend = {
	message: number;
	to: string;
	domain: string | null;
	newDomain: boolean;
	subject: string;
	body: string;
};

/** A file of a reply: numbered from 1. */
export type ReplyFile = {
	index: number;
	name: string;
	kept: boolean;
	bytes: number;
	mediaType?: string;
};

/** A reply from a seller. Everything in it but the numbers is the seller's. */
export type SellerReplyItem = {
	reply: number;
	message: number;
	seller: string;
	/** The subject Farik sent. */
	sentSubject: string;
	from: string;
	subject: string;
	text: string;
	receivedAt: string;
	attachments: ReplyFile[];
	dismissed: boolean;
	order?: number;
};

/** The sentence under a message: the signature and the line saying an AI assistant wrote it. */
export function adds(mailbox: Mailbox | undefined, disclosure: string): string {
	if (!mailbox?.connected) return "";
	return [
		(mailbox.signature ?? "").trim(),
		mailbox.discloseAi === false ? "" : disclosure,
	]
		.filter((part) => part !== "")
		.join("\n\n");
}

/** A time in the browser's own zone: "6 November, 09:41". */
export const timeWords = (when: string): string =>
	new Intl.DateTimeFormat("en-GB", {
		day: "numeric",
		month: "long",
		hour: "2-digit",
		minute: "2-digit",
	}).format(new Date(when));

/** A size in bytes as "812 KB" or "1.4 MB". */
export function sizeWords(bytes: number): string {
	if (bytes < 1_000_000) return `${Math.max(1, Math.round(bytes / 1000))} KB`;
	return `${(bytes / 1_000_000).toFixed(1)} MB`;
}
