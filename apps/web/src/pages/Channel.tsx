import type { Event } from "@farik/protocol-client";
import { Avatar, type AvatarKey, Button, RoleTag } from "@farik/ui";
import { type FormEvent, useEffect, useRef, useState } from "react";
import { Link, useLocation } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useEvents, useQuery } from "../app/store.ts";
import { sentence } from "../app/words.ts";
import { MentionBox } from "../components/MentionBox.tsx";
import {
	renderMessageText,
	type WaitingRow,
} from "../components/MessageText.tsx";
import { useWide } from "../shell/Shell.tsx";
import { t } from "../strings/t.ts";
import styles from "./Channel.module.css";
import { MEETINGS, threadAnchor } from "./SprintPage.tsx";
import type { Agent, Team } from "./setup/TeamSetup.tsx";

/** One message of `channel.messages`, in camelCase. */
type Message = {
	seq: number;
	at: string;
	author: string;
	kind: string;
	text: string;
	mentions: string[];
	thread: string | null;
	inReplyTo: number | null;
	taskId: string | null;
};
type Meeting = { thread: string; at: string };

const PAGE = 100;
/** The most a message holds, in characters, as the runtime counts them (channel.rs). */
const LIMIT = 2000;

/** A `message.posted` event as the query would answer it. */
function fromEvent(e: Event): Message | undefined {
	if (e.kind !== "message.posted") return undefined;
	const body = e.body as Omit<Message, "seq" | "at" | "taskId">;
	return {
		...body,
		thread: body.thread ?? null,
		inReplyTo: body.inReplyTo ?? null,
		seq: e.seq,
		at: e.recordedAt,
		taskId: (e as { taskId?: string }).taskId ?? null,
	};
}

const time = (at: string) => at.slice(11, 16);
const weekday = (at: string) =>
	new Date(at).toLocaleDateString("en-GB", {
		weekday: "long",
		timeZone: "UTC",
	});
const threadWord = (thread: string) => {
	const word = MEETINGS[thread];
	return word ? t(word) : thread;
};

function nameOf(author: string, agents: Agent[]) {
	if (author === "human") return t("channelYou");
	if (author === "farik") return t("brand");
	return agents.find((a) => a.id === author)?.displayName ?? author;
}

type Item =
	| { message: Message }
	| { anchor: string; thread: string; messages: Message[] };

/** Ceremony messages sharing a thread and a UTC day form one block, where the first of them stood. */
function group(messages: Message[]): Item[] {
	const items: Item[] = [];
	const blocks = new Map<string, Message[]>();
	for (const message of messages) {
		if (message.kind !== "ceremony" || !message.thread) {
			items.push({ message });
			continue;
		}
		const anchor = threadAnchor(message.thread, message.at);
		const block = blocks.get(anchor);
		if (block) block.push(message);
		else {
			const started = [message];
			blocks.set(anchor, started);
			items.push({ anchor, thread: message.thread, messages: started });
		}
	}
	return items;
}

/** The team channel: its messages, live, the ceremonies as threads, and a box to post. */
export function Channel() {
	const { client } = useConnection();
	const events = useEvents();
	const wide = useWide();
	const { hash } = useLocation();
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { data: waiting } = useQuery<{ waiting: WaitingRow[] }>(
		"waiting.list",
		{},
	);
	const { data: sprint } = useQuery<{ sprintId: string } | null>(
		"sprint.current",
		{},
	);
	// The pages read, and the newest seq of the first: live events after it are appended.
	const [loaded, setLoaded] = useState<{
		messages: Message[];
		floor: number;
		more: boolean;
	}>();
	const [opened, setOpened] = useState<Record<string, boolean>>({});

	useEffect(() => {
		if (!client) return;
		let live = true;
		client
			.query("channel.messages", { limit: PAGE })
			.then((answer) => {
				const { messages } = answer as { messages: Message[] };
				if (live)
					setLoaded({
						messages,
						floor: messages.at(-1)?.seq ?? 0,
						more: messages.length === PAGE,
					});
			})
			.catch(() => {});
		return () => {
			live = false;
		};
	}, [client]);

	const all = new Map<number, Message>();
	for (const m of loaded?.messages ?? []) all.set(m.seq, m);
	for (const e of events)
		if (loaded && e.seq > loaded.floor) {
			const m = fromEvent(e);
			if (m) all.set(m.seq, m);
		}
	const messages = [...all.values()].sort((a, b) => a.seq - b.seq);

	// A "Read it" link opens its thread and brings it into view, once it is there.
	const scrolled = useRef(false);
	useEffect(() => {
		if (!loaded || scrolled.current || !hash) return;
		scrolled.current = true;
		document.getElementById(hash.slice(1))?.scrollIntoView?.();
	}, [loaded, hash]);

	const earlier = async () => {
		if (!client || !loaded) return;
		const answer = (await client.query("channel.messages", {
			beforeSeq: messages[0]?.seq,
			limit: PAGE,
		})) as { messages: Message[] };
		setLoaded({
			...loaded,
			messages: [...answer.messages, ...loaded.messages],
			more: answer.messages.length === PAGE,
		});
	};

	const agents = team?.team.agents ?? [];
	const rows = waiting?.waiting ?? [];
	const today = new Date().toISOString().slice(0, 10);
	const isOpen = (anchor: string) =>
		opened[anchor] ??
		(anchor === `thread-standup-${today}` || `#${anchor}` === hash);
	const open = (anchor: string, to: boolean) =>
		setOpened((o) => ({ ...o, [anchor]: to }));

	return (
		<div className={styles.page}>
			<div className={styles.main}>
				<div className={styles.head}>
					<h1 className={styles.title}>{t("channel")}</h1>
					<p className={styles.muted}>{t("channelIntro")}</p>
				</div>
				{loaded?.more && (
					<div>
						<Button onClick={earlier}>{t("channelEarlier")}</Button>
					</div>
				)}
				<ol className={styles.messages} aria-label={t("channelMessages")}>
					{group(messages).map((item) =>
						"message" in item ? (
							<MessageRow
								key={item.message.seq}
								message={item.message}
								agents={agents}
								waiting={rows}
								repliedTo={
									item.message.inReplyTo === null
										? undefined
										: all.get(item.message.inReplyTo)
								}
							/>
						) : (
							<ThreadBlock
								key={item.anchor}
								anchor={item.anchor}
								thread={item.thread}
								messages={item.messages}
								agents={agents}
								waiting={rows}
								open={isOpen(item.anchor)}
								onToggle={() => open(item.anchor, !isOpen(item.anchor))}
							/>
						),
					)}
				</ol>
				<Composer agents={agents} wide={wide} />
			</div>
			{sprint && (
				<Meetings
					sprintId={sprint.sprintId}
					onOpen={(anchor) => open(anchor, true)}
				/>
			)}
		</div>
	);
}

function Face({ author, agents }: { author: string; agents: Agent[] }) {
	const agent = agents.find((a) => a.id === author);
	if (author === "human")
		return (
			<span className={styles.you} aria-hidden="true">
				{t("channelYou")}
			</span>
		);
	return agent?.avatar ? (
		<Avatar
			avatarKey={agent.avatar as AvatarKey}
			name={agent.displayName}
			size={48}
		/>
	) : (
		<span className={styles.noFace} />
	);
}

function MessageRow({
	message,
	agents,
	waiting,
	repliedTo,
}: {
	message: Message;
	agents: Agent[];
	waiting: WaitingRow[];
	repliedTo: Message | undefined;
}) {
	const text = renderMessageText(message.text, agents, waiting);
	const stamp = (
		<time className={styles.time} dateTime={message.at}>
			{time(message.at)}
		</time>
	);
	if (message.kind === "system")
		return (
			<li className={styles.system}>
				<strong>{t("brand")}</strong> {text} {stamp}
			</li>
		);
	const agent = agents.find((a) => a.id === message.author);
	const task = message.taskId;
	return (
		<li className={message.author === "human" ? styles.mine : styles.message}>
			<Face author={message.author} agents={agents} />
			<div className={styles.body}>
				<p className={styles.meta}>
					<strong>{nameOf(message.author, agents)}</strong>
					{agent && <RoleTag role={agent.role} />}
					{stamp}
				</p>
				{repliedTo && (
					<p className={styles.reply}>
						{t("channelReplyingTo", {
							name: nameOf(repliedTo.author, agents),
						})}
					</p>
				)}
				<p className={styles.text}>
					{text}
					{task && !message.text.includes(task) && (
						<> {renderMessageText(task, agents, waiting)}</>
					)}
				</p>
			</div>
		</li>
	);
}

function ThreadBlock({
	anchor,
	thread,
	messages,
	agents,
	waiting,
	open,
	onToggle,
}: {
	anchor: string;
	thread: string;
	messages: Message[];
	agents: Agent[];
	waiting: WaitingRow[];
	open: boolean;
	onToggle: () => void;
}) {
	const people = new Set(messages.map((m) => m.author)).size;
	const fill = {
		thread: threadWord(thread),
		weekday: weekday(messages[0]?.at ?? ""),
		n: people,
	};
	return (
		<li id={anchor} className={styles.thread}>
			<button
				type="button"
				className={styles.toggle}
				aria-expanded={open}
				onClick={onToggle}
			>
				<span>
					{t(people === 1 ? "channelThreadOne" : "channelThread", fill)}
				</span>
				<span className={styles.muted}>
					{t(open ? "channelHide" : "channelShow")}
				</span>
			</button>
			{open && (
				<ul className={styles.posts}>
					{messages.map((m) => {
						const agent = agents.find((a) => a.id === m.author);
						return (
							<li key={m.seq}>
								{agent?.avatar && (
									<Avatar
										avatarKey={agent.avatar as AvatarKey}
										name={agent.displayName}
										size={32}
									/>
								)}
								<span>
									<strong>{nameOf(m.author, agents)}</strong>{" "}
									{renderMessageText(m.text, agents, waiting)}
								</span>
							</li>
						);
					})}
				</ul>
			)}
		</li>
	);
}

/** "Post to the team": `message_post { text }`, refused here when it is too long. */
function Composer({ agents, wide }: { agents: Agent[]; wide: boolean }) {
	const { client } = useConnection();
	const [text, setText] = useState("");
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const post = async (e: FormEvent) => {
		e.preventDefault();
		if (!client) return;
		// Counted in code points, as the runtime's `chars().count()`.
		const length = [...text].length;
		if (length > LIMIT) return setRefusal(t("channelTooLong", { length }));
		setBusy(true);
		setRefusal(undefined);
		try {
			const reply = await client.command({
				command: "message_post",
				body: { text },
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
		<form className={styles.composer} onSubmit={post}>
			<label
				htmlFor="channel-post"
				className={wide ? styles.label : styles.hidden}
			>
				{t(wide ? "channelPostLabel" : "channelPostPhone")}
			</label>
			<div className={styles.row}>
				<MentionBox
					id="channel-post"
					value={text}
					onChange={setText}
					agents={agents}
					describedBy="channel-hint"
				/>
				<Button kind="primary" type="submit" busy={busy}>
					{t("channelPost")}
				</Button>
			</div>
			<p id="channel-hint" className={styles.muted}>
				{t("channelHintLead")} <Link to="/">{t("channelHintLink")}</Link>{" "}
				{t("channelHintEnd")}
			</p>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
		</form>
	);
}

/** "Meetings in this sprint": a link to each of the open sprint's thread blocks. */
function Meetings({
	sprintId,
	onOpen,
}: {
	sprintId: string;
	onOpen: (anchor: string) => void;
}) {
	const { data } = useQuery<{ meetings: Meeting[] }>("sprint.get", {
		sprintId,
	});
	return (
		<aside className={styles.side} aria-labelledby="channel-meetings">
			<h2 id="channel-meetings">{t("channelMeetings")}</h2>
			{data?.meetings.map((m) => {
				const anchor = threadAnchor(m.thread, m.at);
				return (
					<a key={anchor} href={`#${anchor}`} onClick={() => onOpen(anchor)}>
						{t("channelMeetingLink", {
							thread: threadWord(m.thread),
							weekday: weekday(m.at),
						})}
					</a>
				);
			})}
		</aside>
	);
}

/** Today's "In the channel": the last two messages that are not Farik's own lines. */
export function ChannelPreview({ agents }: { agents: Agent[] }) {
	// ponytail: the last 20 hold two that are not system lines unless Farik said 19 things in a row.
	const { data } = useQuery<{ messages: Message[] }>("channel.messages", {
		limit: 20,
	});
	if (!data) return null;
	const last = data.messages.filter((m) => m.kind !== "system").slice(-2);
	return (
		<section className={styles.preview} aria-labelledby="channel-preview">
			<h2 id="channel-preview">{t("channelPreview")}</h2>
			{last.map((m) => {
				const agent = agents.find((a) => a.id === m.author);
				return (
					<div key={m.seq} className={styles.said}>
						{agent?.avatar && (
							<Avatar
								avatarKey={agent.avatar as AvatarKey}
								name={agent.displayName}
								size={32}
							/>
						)}
						<p>
							<strong>{nameOf(m.author, agents)}</strong>{" "}
							{renderMessageText(m.text, agents, [])}
						</p>
					</div>
				);
			})}
			<Link to="/channel">{t("channelOpen")}</Link>
		</section>
	);
}
