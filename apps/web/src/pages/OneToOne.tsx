import type { Event } from "@farik/protocol-client";
import { Avatar, type AvatarKey, Button, RoleTag } from "@farik/ui";
import {
	type FormEvent,
	type ReactNode,
	useEffect,
	useRef,
	useState,
} from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { daemonSaid, saidAll } from "../app/refusals.ts";
import { useEvents } from "../app/store.ts";
import { codeOf, sentence } from "../app/words.ts";
import { sendOnEnter } from "../components/MentionBox.tsx";
import { renderMessageText } from "../components/MessageText.tsx";
import { useWide } from "../shell/Shell.tsx";
import { t } from "../strings/t.ts";
import channel from "./Channel.module.css";
import styles from "./Chats.module.css";
import type { Agent } from "./setup/TeamSetup.tsx";

type Proposal = { title: string; text: string };
/** One message of `chat.messages`, in camelCase. */
type ChatMessage = {
	seq: number;
	at: string;
	author: string;
	text: string;
	inReplyTo: number | null;
	request: Proposal | null;
	sentAs: string | null;
};
type Because =
	| "retired"
	| "key_refused"
	| "day_spent"
	| "asleep"
	| "answering"
	| "no_answer";
type Waiting = { because: Because; until?: string } | null;

/** A message's time in UTC, as the channel shows it: today's as 09:44, an older one's weekday first. */
export function when(at: string, weekdayOnly = false): string {
	const today = new Date().toISOString().slice(0, 10);
	const hour = at.slice(11, 16);
	if (at.slice(0, 10) === today) return hour;
	const day = new Date(at).toLocaleDateString("en-GB", {
		weekday: "short",
		timeZone: "UTC",
	});
	return weekdayOnly ? day : `${day} ${hour}`;
}

/** Who wrote a line: You, or the agent's name. */
export const authorName = (author: string, agents: Agent[]) =>
	author === "human"
		? t("channelYou")
		: (agents.find((a) => a.id === author)?.displayName ?? author);

const PAGE = 100;
/** The most a chat message holds, in characters, as the runtime counts them (chat.rs). */
const LIMIT = 4000;

const bySeq = (a: ChatMessage[], b: ChatMessage[]) => {
	const all = new Map([...a, ...b].map((m) => [m.seq, m]));
	return [...all.values()].sort((x, y) => x.seq - y.seq);
};

/** A one-to-one chat with `agent`: its history, live, why it has no answer yet, and a box to write. */
export function OneToOne({ agent, agents }: { agent: Agent; agents: Agent[] }) {
	const { client } = useConnection();
	const events = useEvents();
	const wide = useWide();
	const [messages, setMessages] = useState<ChatMessage[]>([]);
	const [waiting, setWaiting] = useState<Waiting>(null);
	const [round, setRound] = useState(0);
	const id = agent.id;
	const name = agent.displayName;

	// biome-ignore lint/correctness/useExhaustiveDependencies: round asks for the page again
	useEffect(() => {
		if (!client) return;
		let live = true;
		client
			.query("chat.messages", { agentId: id, limit: PAGE })
			.then((answer) => {
				const page = answer as { messages: ChatMessage[]; waiting: Waiting };
				if (!live) return;
				setMessages((m) => bySeq(m, page.messages));
				setWaiting(page.waiting);
			})
			.catch(() => {});
		return () => {
			live = false;
		};
	}, [client, id, round]);

	// Events since the page opened, each read once: this chat's messages are added as they come,
	// and its sessions starting or ending ask again why it waits.
	const read = useRef(new Set(events.map((e) => e.seq)));
	useEffect(() => {
		const fresh = events.filter((e) => !read.current.has(e.seq));
		if (!fresh.length) return;
		for (const e of fresh) read.current.add(e.seq);
		const added = fresh.flatMap((e) => {
			const m = fromEvent(e, id);
			return m ? [m] : [];
		});
		const sent = new Map(
			fresh.flatMap((e) => {
				const from = (e.body as { fromChatMessage?: number }).fromChatMessage;
				return e.kind === "task.created" && from && e.taskId
					? [[from, e.taskId] as const]
					: [];
			}),
		);
		if (added.length || sent.size)
			setMessages((m) =>
				bySeq(m, added).map((x) =>
					sent.has(x.seq) ? { ...x, sentAs: sent.get(x.seq) ?? null } : x,
				),
			);
		if (added.length || fresh.some((e) => asksAgain(e, id)))
			setRound((n) => n + 1);
	}, [events, id]);

	const retired = agent.status === "retired";
	const because = retired ? "retired" : waiting?.because;
	const markSent = (seq: number, taskId: string) =>
		setMessages((m) =>
			m.map((x) => (x.seq === seq ? { ...x, sentAs: taskId } : x)),
		);

	return (
		<div className={styles.chat}>
			<div className={styles.head}>
				{agent.avatar && (
					<Avatar avatarKey={agent.avatar as AvatarKey} name={name} size={48} />
				)}
				<div className={styles.heading}>
					<div className={styles.titled}>
						<h1 className={channel.title}>{t("chatsTitle", { name })}</h1>
						<RoleTag role={agent.role} />
					</div>
					<p className={channel.muted}>
						{t(wide ? "chatsIntro" : "chatsIntroPhone", { name })}
					</p>
				</div>
				<Link to="/channel" className={styles.back}>
					{t("chatsBack")}
				</Link>
			</div>
			<ol className={channel.messages} aria-label={t("channelMessages")}>
				{messages.map((m) => (
					<Row
						key={m.seq}
						message={m}
						agents={agents}
						onSent={(taskId) => markSent(m.seq, taskId)}
					/>
				))}
			</ol>
			<p role="status" className={styles.waiting}>
				{because && agent.avatar && (
					<span aria-hidden="true">
						<Avatar
							avatarKey={agent.avatar as AvatarKey}
							name={name}
							size={32}
						/>
					</span>
				)}
				{because && <span>{waitingWords(because, name, waiting?.until)}</span>}
			</p>
			{!retired && <Composer agent={agent} />}
		</div>
	);
}

/** A `chat_message.posted` of this chat, as the query would answer it. */
function fromEvent(e: Event, chat: string): ChatMessage | undefined {
	if (e.kind !== "chat_message.posted") return undefined;
	const body = e.body as {
		chat: string;
		author: string;
		text: string;
		inReplyTo?: number;
		request?: Proposal;
	};
	if (body.chat !== chat) return undefined;
	return {
		seq: e.seq,
		at: e.recordedAt,
		author: body.author,
		text: body.text,
		inReplyTo: body.inReplyTo ?? null,
		request: body.request ?? null,
		sentAs: null,
	};
}

/** Whether `e` may change why this chat waits: one of its chat sessions starting, or ending. */
function asksAgain(e: Event, chat: string): boolean {
	const body = e.body as { purpose?: string; chat?: string };
	if (e.kind === "session.started")
		return body.purpose === "chat" && body.chat === chat;
	// ponytail: session.ended names no purpose, so any of the agent's sessions ending asks again;
	// a cheap query. Match by session id if that ever costs.
	return e.kind === "session.ended" && e.agentId === chat;
}

/** The line above the box: why the agent has not answered yet. */
function waitingWords(
	because: Because,
	name: string,
	until: string | undefined,
): ReactNode {
	switch (because) {
		case "answering":
			return t("chatsAnswering", { name });
		case "day_spent":
			return (
				<>
					{t("chatsDaySpent", { name })}{" "}
					<Link to="/costs">{t("chatsDaySpentLink")}</Link>
				</>
			);
		case "asleep": {
			const time = new Date(until ?? "")
				.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" })
				.toLowerCase();
			return t("chatsAsleep", { name, time });
		}
		case "key_refused":
			return (
				<>
					{t("chatsKeyRefused", { name })}{" "}
					<Link to="/settings">{t("chatsKeyLink")}</Link>
				</>
			);
		case "no_answer":
			return t("chatsNoAnswer", { name });
		case "retired":
			return t("chatsRetired", { name });
	}
}

function Row({
	message,
	agents,
	onSent,
}: {
	message: ChatMessage;
	agents: Agent[];
	onSent: (taskId: string) => void;
}) {
	const mine = message.author === "human";
	const agent = agents.find((a) => a.id === message.author);
	return (
		<li className={mine ? channel.mine : channel.message}>
			{mine ? (
				<span className={channel.you} aria-hidden="true">
					{t("channelYou")}
				</span>
			) : agent?.avatar ? (
				<Avatar
					avatarKey={agent.avatar as AvatarKey}
					name={agent.displayName}
					size={48}
				/>
			) : (
				<span className={channel.noFace} />
			)}
			<div className={channel.body}>
				<p className={channel.meta}>
					<strong>{authorName(message.author, agents)}</strong>
					{agent && <RoleTag role={agent.role} />}
					<time className={channel.time} dateTime={message.at}>
						{when(message.at)}
					</time>
				</p>
				<p className={`${channel.text} ${styles.lines}`}>
					{renderMessageText(message.text, agents, [])}
				</p>
				{message.request && (
					<ProposalBox
						seq={message.seq}
						request={message.request}
						sentAs={message.sentAs}
						name={authorName(message.author, agents)}
						onSent={onSent}
					/>
				)}
			</div>
		</li>
	);
}

/** The reply's proposed request: its words, editable, until "Send as a request" files them; then the link. */
function ProposalBox({
	seq,
	request,
	sentAs,
	name,
	onSent,
}: {
	seq: number;
	request: Proposal;
	sentAs: string | null;
	name: string;
	onSent: (taskId: string) => void;
}) {
	const { client } = useConnection();
	const [text, setText] = useState(`${request.title}\n\n${request.text}`);
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	if (sentAs)
		return (
			<div className={styles.proposal}>
				<p className={channel.muted}>{t("chatsSuggested", { name })}</p>
				<p className={styles.lines}>
					<strong>{request.title}</strong>
				</p>
				<Link to={`/requests/${sentAs}`}>
					{t("chatsSentAs", { id: sentAs })}
				</Link>
			</div>
		);
	const send = async (e: FormEvent) => {
		e.preventDefault();
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const filed = (await client.call("request.file", {
				text,
				fromChatMessage: seq,
			})) as { taskId: string };
			onSent(filed.taskId);
		} catch (error) {
			setRefusal(
				codeOf(error) ? saidAll(error) : daemonSaid(error, "refuseCommand"),
			);
		} finally {
			setBusy(false);
		}
	};
	const box = `proposal-${seq}`;
	return (
		<form className={styles.proposal} onSubmit={send}>
			<label htmlFor={box} className={styles.label}>
				{t("chatsSuggests", { name })}
			</label>
			<textarea
				id={box}
				rows={4}
				className={styles.input}
				value={text}
				onChange={(e) => setText(e.target.value)}
			/>
			<div className={styles.send}>
				<Button kind="primary" type="submit" busy={busy}>
					{t("chatsSendRequest")}
				</Button>
				<span className={channel.muted}>{t("chatsNothingSent")}</span>
			</div>
			{refusal && (
				<p role="alert" className={channel.alert}>
					{refusal}
				</p>
			)}
		</form>
	);
}

/** "Message <Name>": `chat_message_post { agent_id, text }`, refused here when it is too long. */
function Composer({ agent }: { agent: Agent }) {
	const { client } = useConnection();
	const [text, setText] = useState("");
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const post = async (e: FormEvent) => {
		e.preventDefault();
		if (!client) return;
		// Counted in code points, as the runtime's `chars().count()`.
		const length = [...text].length;
		if (length > LIMIT) return setRefusal(t("chatsTooLong", { length }));
		setBusy(true);
		setRefusal(undefined);
		try {
			const reply = await client.command({
				command: "chat_message_post",
				body: { agentId: agent.id, text },
			});
			if ("error" in reply) setRefusal(sentence(reply.error.detail));
			else setText("");
		} catch {
			// The connection closed: the page shows that it is lost.
		} finally {
			setBusy(false);
		}
	};
	return (
		<form className={channel.composer} onSubmit={post}>
			<label htmlFor="chat-post" className={channel.label}>
				{t("chatsLabel", { name: agent.displayName })}
			</label>
			<div className={channel.row}>
				<textarea
					id="chat-post"
					rows={2}
					className={styles.input}
					value={text}
					aria-describedby="chat-enter"
					onChange={(e) => setText(e.target.value)}
					onKeyDown={sendOnEnter}
				/>
				<Button kind="primary" type="submit" busy={busy}>
					{t("chatsSend")}
				</Button>
			</div>
			<p id="chat-enter" className={channel.muted}>
				{t("chatsEnterSends")}
			</p>
			{refusal && (
				<p role="alert" className={channel.alert}>
					{refusal}
				</p>
			)}
		</form>
	);
}
