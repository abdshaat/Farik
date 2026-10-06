import { Avatar, type AvatarKey, Button, Dialog } from "@farik/ui";
import { type ReactNode, useEffect, useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { t } from "../strings/t.ts";
import { useCommand } from "./dialogs/StartSprint.tsx";
import { channelName } from "./marketing.ts";
import styles from "./PostGoingOut.module.css";
import type { Agent } from "./setup/TeamSetup.tsx";

/** One picture or clip of a post. */
export type PostMedia = { url: string; kind: "image" | "video" };

/** A post as `social_posts.list` answers it, in camelCase. */
export type GoingOutPost = {
	post: number;
	agentId: string;
	channel: string;
	text: string;
	media: PostMedia[];
	at: string;
	handsOverAt: string;
	state: "scheduled" | "sent" | "missed" | "failed";
	plan?: string;
	slot?: string;
	approvedBy?: "plan" | "owner";
	missedWhy?: "not_running" | "paused" | "undecided";
	reason?: string;
};

/** The side of a picture's thumbnail on Today, in pixels (a phone draws it smaller, by its style). */
const THUMBNAIL = 88;
const MINUTE_MS = 60_000;
const DAY_MS = 86_400_000;

/** The parts of `time`, in the browser's own time zone, as `Intl` words them, by type. */
function local(
	time: Date,
	options: Intl.DateTimeFormatOptions,
): Record<string, string> {
	return Object.fromEntries(
		new Intl.DateTimeFormat("en-GB", { ...options, hourCycle: "h23" })
			.formatToParts(time)
			.map((part) => [part.type, part.value]),
	);
}

/** "13:00", in the browser's time zone. */
export function clock(time: Date): string {
	const p = local(time, { hour: "2-digit", minute: "2-digit" });
	return `${p.hour}:${p.minute}`;
}

/** "Wednesday 28 October". */
export function longDate(time: Date): string {
	const p = local(time, { weekday: "long", day: "numeric", month: "long" });
	return `${p.weekday} ${p.day} ${p.month}`;
}

/** "Tue 13 Oct". */
export function shortDate(time: Date): string {
	const p = local(time, { weekday: "short", day: "numeric", month: "short" });
	return `${p.weekday} ${p.day} ${p.month}`;
}

/** How many calendar days, in the browser's time zone, `time` is after `now`. */
function daysAfter(time: Date, now: Date): number {
	const day = (date: Date) =>
		Date.UTC(date.getFullYear(), date.getMonth(), date.getDate());
	return Math.round((day(time) - day(now)) / DAY_MS);
}

/** "today", "tomorrow", "yesterday", else "Wednesday 28 October". */
export function dayWords(time: Date, now: Date): string {
	const days = daysAfter(time, now);
	if (days === 0) return t("postToday");
	if (days === 1) return t("postTomorrow");
	if (days === -1) return t("postYesterday");
	return longDate(time);
}

/**
 * "in 45 minutes", "in 3 hours 45 minutes", "in 2 days": minutes (a part of one is one) up to a
 * day, then the days of the calendar. A post whose time has come is "in 1 minute".
 */
export function howSoon(time: Date, now: Date): string {
	const minutes = Math.max(
		1,
		Math.ceil((time.getTime() - now.getTime()) / MINUTE_MS),
	);
	if (minutes >= 24 * 60) {
		const days = Math.max(1, daysAfter(time, now));
		return t("postIn", {
			span: days === 1 ? t("postDay") : t("postDays", { n: days }),
		});
	}
	const hours = Math.floor(minutes / 60);
	const rest = minutes % 60;
	const span = [
		hours > 0 && (hours === 1 ? t("postHour") : t("postHours", { n: hours })),
		rest > 0 && (rest === 1 ? t("postMinute") : t("postMinutes", { n: rest })),
	]
		.filter((part): part is string => part !== false)
		.join(" ");
	return t("postIn", { span });
}

/** The badge a network gets. */
const BADGES: Record<string, string> = {
	instagram: "Ig",
	x: "X",
	facebook: "Fb",
	linkedin: "in",
	threads: "Th",
	bluesky: "Bs",
	tiktok: "Tt",
	pinterest: "Pi",
	youtube: "Yt",
	google_business: "Gb",
	mastodon: "Ma",
};

/** "Instagram, today at 13:00". */
export function postWhen(channel: string, time: Date, now: Date): string {
	return t("postWhen", {
		network: channelName(channel),
		day: dayWords(time, now),
		time: clock(time),
	});
}

/** One picture the daemon fetches, or a link to open it where it is when it will not show. */
function Picture({
	post,
	index,
	url,
}: {
	post: number;
	index: number;
	url: string;
}) {
	const { client } = useConnection();
	const [shown, setShown] = useState<
		{ mediaType: string; base64: string } | "opens" | undefined
	>();
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.call("social_post.media", { post, index }).then(
			(answer) =>
				live && setShown(answer as { mediaType: string; base64: string }),
			() => live && setShown("opens"),
		);
		return () => {
			live = false;
		};
	}, [client, post, index]);
	if (shown === undefined) return null;
	if (shown === "opens")
		return <OpensInATab url={url} word="postOpenPicture" />;
	return (
		<img
			alt={t("postPictureAlt", { n: index + 1 })}
			src={`data:${shown.mediaType};base64,${shown.base64}`}
			width={THUMBNAIL}
			height={THUMBNAIL}
		/>
	);
}

/** A link to an address, in a new tab that cannot reach this page; none for an address that is not `https`. */
function OpensInATab({
	url,
	word,
}: {
	url: string;
	word: "postWatchClip" | "postOpenPicture";
}) {
	if (!url.startsWith("https://")) return null;
	return (
		<a
			className={styles.tile}
			href={url}
			target="_blank"
			rel="noopener noreferrer"
		>
			{word === "postWatchClip" && (
				<svg
					width="22"
					height="22"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2"
					aria-hidden="true"
				>
					<circle cx="12" cy="12" r="10" />
					<path d="M10 8l6 4-6 4z" fill="currentColor" />
				</svg>
			)}
			{t(word)}
		</a>
	);
}

/** The pictures and clips of a post with their places, and a key each that no other of the post has. */
function keyed(media: PostMedia[]) {
	return media.map((one, index) => ({
		one,
		index,
		key: `${one.url}#${media.slice(0, index).filter((other) => other.url === one.url).length}`,
	}));
}

/**
 * A post's badge, its network and time, its words as typed, and its pictures: the parts of one
 * column, for its caller to put in a box. `clamp` cuts the words after two lines, for a post that
 * is over; the page holds all of them.
 */
export function PostBody({
	post,
	now,
	titleId,
	clamp,
	children,
}: {
	post: Pick<GoingOutPost, "post" | "channel" | "text" | "media" | "at">;
	now: Date;
	/** The id of the line that names the post, for the button that acts on it. */
	titleId?: string;
	clamp?: boolean;
	/** What goes beside the time. */
	children?: ReactNode;
}) {
	return (
		<>
			<span className={styles.head}>
				<span className={styles.badge} aria-hidden="true">
					{BADGES[post.channel] ?? post.channel.slice(0, 2)}
				</span>
				<span className={styles.when}>
					<strong id={titleId}>
						{postWhen(post.channel, new Date(post.at), now)}
					</strong>
					{children}
				</span>
			</span>
			{/* What the agent wrote: React shows it as text, never as markup. */}
			<p className={clamp ? `${styles.text} ${styles.clamp}` : styles.text}>
				{post.text}
			</p>
			{post.media.length > 0 && (
				<div className={styles.pictures}>
					{keyed(post.media).map(({ key, index, one }) =>
						one.kind === "video" ? (
							<OpensInATab key={key} url={one.url} word="postWatchClip" />
						) : (
							<Picture key={key} post={post.post} index={index} url={one.url} />
						),
					)}
				</div>
			)}
		</>
	);
}

/** A post as its own block: its badge, its network and time, its words and its pictures. */
export function PostCard({
	post,
	now,
}: {
	post: Pick<GoingOutPost, "post" | "channel" | "text" | "media" | "at">;
	now: Date;
}) {
	return (
		<div className={styles.card}>
			<PostBody post={post} now={now} />
		</div>
	);
}

/** The agent who wrote a post, by its picture; none when the team does not say which. */
function Writer({ agent, name }: { agent: Agent | undefined; name: string }) {
	if (!agent?.avatar) return null;
	return <Avatar avatarKey={agent.avatar as AvatarKey} name={name} size={32} />;
}

/** The posts going out, soonest first, and the ones that did not go out in the last day. */
export function GoingOut({
	posts,
	agents,
	now,
	again,
}: {
	posts: GoingOutPost[];
	agents: Agent[];
	now: Date;
	/** Asks for the list again, once a post is stopped. */
	again: () => void;
}) {
	const going = posts.filter(
		(post) => post.state === "scheduled" || post.state === "sent",
	);
	const over = posts.filter(
		(post) => post.state === "missed" || post.state === "failed",
	);
	const writer = (id: string) => agents.find((agent) => agent.id === id);
	const name = (id: string) => writer(id)?.displayName ?? id;
	return (
		<>
			{going.length > 0 && (
				<section className={styles.section} aria-labelledby="going-out-heading">
					<h2 id="going-out-heading">
						{t("goingOutTitle", { count: going.length })}
					</h2>
					<p className={styles.why}>{t("goingOutLead")}</p>
					<ul className={styles.rows} aria-label={t("goingOutList")}>
						{going.map((post) => (
							<GoingOutRow
								key={post.post}
								post={post}
								agent={writer(post.agentId)}
								name={name(post.agentId)}
								now={now}
								again={again}
							/>
						))}
					</ul>
				</section>
			)}
			{over.length > 0 && (
				<section
					className={styles.section}
					aria-labelledby="did-not-go-out-heading"
				>
					<h2 id="did-not-go-out-heading">{t("didNotGoOutTitle")}</h2>
					<ul className={styles.rows} aria-label={t("didNotGoOutList")}>
						{over.map((post) => (
							<li key={post.post} className={`${styles.row} ${styles.over}`}>
								<Writer
									agent={writer(post.agentId)}
									name={name(post.agentId)}
								/>
								<div className={styles.rowText}>
									<PostBody post={post} now={now} clamp>
										<span className={styles.state}>
											{t(post.state === "failed" ? "postFailed" : "postMissed")}
										</span>
									</PostBody>
									{/* Buffer's words, or Farik's: text, never markup. */}
									<p className={styles.why}>
										{post.state === "failed"
											? post.reason
											: t(
													post.missedWhy === "paused"
														? "postMissedPaused"
														: post.missedWhy === "undecided"
															? "postMissedUndecided"
															: "postMissedNotRunning",
												)}{" "}
										{t("postHears", { name: name(post.agentId) })}
									</p>
								</div>
							</li>
						))}
					</ul>
				</section>
			)}
		</>
	);
}

/** Why a post goes out, and when Farik hands it to Buffer. */
function Why({ post, now }: { post: GoingOutPost; now: Date }) {
	const handsOver = new Date(post.handsOverAt);
	const ending =
		post.state === "sent"
			? t("postBufferHas", { time: clock(new Date(post.at)) })
			: daysAfter(handsOver, now) === 0
				? t("postHandsOverAt", { time: clock(handsOver) })
				: t("postHandsOverBefore");
	return (
		<p className={styles.why}>
			{post.approvedBy === "owner" || !post.plan ? (
				t("postAllowed")
			) : (
				<>
					{t("postApprovedBefore")}
					<Link to={`/marketing/plans/${post.plan}`}>{post.plan}</Link>
				</>
			)}
			{ending}
		</p>
	);
}

function GoingOutRow({
	post,
	agent,
	name,
	now,
	again,
}: {
	post: GoingOutPost;
	agent: Agent | undefined;
	name: string;
	now: Date;
	again: () => void;
}) {
	const [stopping, setStopping] = useState(false);
	const titleId = `going-out-${post.post}`;
	return (
		<li className={styles.row}>
			<Writer agent={agent} name={name} />
			<div className={styles.rowText}>
				<PostBody post={post} now={now} titleId={titleId}>
					<span className={styles.soon}>{howSoon(new Date(post.at), now)}</span>
				</PostBody>
				<Why post={post} now={now} />
			</div>
			<button
				type="button"
				className={styles.stopButton}
				aria-describedby={titleId}
				onClick={() => setStopping(true)}
			>
				{t("postStop")}
			</button>
			{stopping && (
				<StopPost
					post={post}
					name={name}
					now={now}
					onClose={() => setStopping(false)}
					onStopped={() => {
						setStopping(false);
						again();
					}}
				/>
			)}
		</li>
	);
}

/** "Stop this post?": what stopping does, then the stop, and what to do when Buffer will not give it back. */
function StopPost({
	post,
	name,
	now,
	onClose,
	onStopped,
}: {
	post: GoingOutPost;
	name: string;
	now: Date;
	onClose: () => void;
	onStopped: () => void;
}) {
	const { busy, refusal, code, send } = useCommand(onStopped, {
		time: clock(new Date(post.at)),
	});
	return (
		<Dialog
			open
			title={t("stopTitle")}
			onClose={onClose}
			actions={
				<Button
					kind="primary"
					busy={busy}
					onClick={() =>
						send({ command: "social_post_stop", body: { post: post.post } })
					}
				>
					{t("stopConfirm")}
				</Button>
			}
		>
			<div className={styles.dialog}>
				<PostCard post={post} now={now} />
				<p>{t("stopNotGoOut")}</p>
				{post.state === "sent" ? (
					<p>{t("stopTakesBack")}</p>
				) : (
					post.plan && <p>{t("stopFreesDay", { name })}</p>
				)}
				{refusal && <p role="alert">{refusal}</p>}
				{code === "post_not_taken_back" && (
					<p>
						<a
							href="https://publish.buffer.com"
							target="_blank"
							rel="noopener noreferrer"
						>
							{t("stopOpenBuffer")}
						</a>
					</p>
				)}
			</div>
		</Dialog>
	);
}
