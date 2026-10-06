import { Avatar, type AvatarKey, Button, Dialog, TextArea } from "@farik/ui";
import { Fragment, type ReactNode, useState } from "react";
import { Link, useParams } from "react-router";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { useCommand } from "./dialogs/StartSprint.tsx";
import { Failed } from "./Failed.tsx";
import own from "./MarketingPlan.module.css";
import {
	channelName,
	currencyWords,
	dayOf,
	length,
	listed,
	longDay,
	money,
	runs,
	shortDay,
	shortRange,
	today,
	weeks,
	when,
} from "./marketing.ts";
import { clock } from "./PostGoingOut.tsx";
import styles from "./pages.module.css";
import {
	endingPosts,
	slotStanding,
	standingCounts,
	standingDetail,
	standingWord,
	type WrittenPost,
} from "./planPosts.ts";
import type { Team } from "./setup/TeamSetup.tsx";

type Campaign = {
	key: string;
	name: string;
	goal: string;
	budget: string;
	startsOn: string;
	endsOn: string;
};
type Post = { key: string; channel: string; on: string; topic: string };
type State = "proposed" | "returned" | "approved" | "active" | "ended";
/** A plan as `marketing_plan.get` answers it, in camelCase. */
type Plan = {
	plan: string;
	title: string;
	summary: string;
	text: string;
	state: State;
	startsOn: string;
	endsOn: string;
	currency: string;
	budget: { total: string; googleAds: string };
	campaigns: Campaign[];
	posts: Post[];
	/** What was written for its slots, oldest first. */
	writtenPosts?: WrittenPost[];
	measures: string[];
	googleAdsAccount: string | null;
	agentId: string;
	taskId: string;
	proposedAt: string;
	decided: {
		decision: "approved" | "returned";
		note?: string;
		reason?: string;
		at: string;
	} | null;
	ended: {
		why: "replaced" | "by_owner" | "expired";
		at: string;
		/** What the owner said when they ended it. */
		note?: string;
		/** The plan that took its place. */
		replacedBy?: string;
	} | null;
};

/** What the daemon takes in a note. */
const MAX_NOTE = 600;

/** The characters of `words` the daemon counts. */
const counted = (words: string) => [...words.trim()].length;

/** How many posts there are, in words. */
const postsWord = (n: number) =>
	n === 1 ? t("marketingPostOne") : t("marketingPostsCount", { n });

/** A marketing plan to read and decide: its budget, its calendar, and the agent's own text. */
export function MarketingPlan() {
	const { id = "" } = useParams();
	const { data: team, error: e1 } = useQuery<{ team: Team }>("team.get", {});
	const {
		data: plan,
		error: e2,
		again,
	} = useQuery<Plan>("marketing_plan.get", { plan: id });
	const [asking, setAsking] = useState<"back" | "end">();
	const decide = useCommand(again);
	if (!plan || (!team && !e1)) return e2 ? <Failed error={e2} /> : null;

	const agent = team?.team.agents.find((one) => one.id === plan.agentId);
	const name = agent?.displayName ?? plan.agentId;
	const now = today();
	const proposed = plan.state === "proposed";
	const live = plan.state === "approved" || plan.state === "active";
	const cash = (amount: string) => money(amount, plan.currency);
	const written = plan.writtenPosts ?? [];
	const instant = new Date();
	const counts = standingCounts(
		written,
		plan.posts.map((post) => post.key),
	);
	const channels = [...new Set(plan.posts.map((post) => post.channel))];
	const breakdown = channels
		.map(
			(channel) =>
				`${plan.posts.filter((post) => post.channel === channel).length} on ${channelName(channel)}`,
		)
		.join(", ");
	const ads = plan.campaigns.length;
	const what = [
		plan.posts.length > 0 &&
			(plan.posts.length === 1
				? t("marketingDoPostsOne")
				: t("marketingDoPosts", { n: plan.posts.length })),
		ads > 0 && t("marketingDoAds", { amount: cash(plan.budget.googleAds) }),
	].filter((one): one is string => one !== false);

	return (
		<div className={own.page}>
			<div className={own.layout}>
				<div className={own.head}>
					<Link to="/">{t("backToToday")}</Link>
					<Header plan={plan} name={name} now={now} />
				</div>
				<aside className={own.side} aria-labelledby="about">
					<h2 id="about" className={own.aboutTitle}>
						{t("planAbout")}
					</h2>
					<dl className={own.facts}>
						<Fact term={t("marketingFactPlan")} extra>
							{plan.plan}
						</Fact>
						<Fact term={t("marketingFactFrom")} extra>
							{t("marketingFactFromValue", { name })}
							<Link to={`/tasks/${plan.taskId}`}>{plan.taskId}</Link>
						</Fact>
						<Fact term={t("marketingFactRuns")}>
							{t("marketingFactRunsValue", {
								from: longDay(plan.startsOn),
								to: longDay(plan.endsOn),
								year: plan.endsOn.slice(0, 4),
								length: runs(plan.startsOn, plan.endsOn),
							})}
						</Fact>
						<Fact term={t("marketingFactBudget")}>
							{t("marketingTotalAmount", {
								amount: cash(plan.budget.total),
								currency: plan.currency,
							})}
						</Fact>
						{ads > 0 && (
							<Fact term={t("marketingFactAds")}>
								{plan.googleAdsAccount
									? t("marketingFactAdsValue", {
											campaigns:
												ads === 1
													? t("marketingCampaignOne")
													: t("marketingCampaigns", { n: ads }),
											account: plan.googleAdsAccount,
										})
									: ads === 1
										? t("marketingCampaignOne")
										: t("marketingCampaigns", { n: ads })}
							</Fact>
						)}
						{plan.posts.length > 0 && (
							<Fact term={t("marketingFactPosts")}>
								{`${plan.posts.length}: ${breakdown}`}
							</Fact>
						)}
						<Fact term={t("marketingFactProposed")} extra>
							{when(plan.proposedAt)}
						</Fact>
						{plan.decided && (
							<Fact
								term={t(
									plan.decided.decision === "approved"
										? "marketingFactApproved"
										: "marketingFactSentBack",
								)}
								extra
							>
								{t("marketingByYou", { at: when(plan.decided.at) })}
							</Fact>
						)}
						{plan.ended && (
							<Fact term={t("marketingFactEnded")} extra>
								{plan.ended.why === "by_owner"
									? t("marketingByYou", { at: when(plan.ended.at) })
									: t(
											plan.ended.why === "expired"
												? "marketingEndedFact"
												: "marketingReplacedFact",
											{ day: longDay(plan.ended.at) },
										)}
							</Fact>
						)}
					</dl>
				</aside>
				<div className={own.main}>
					<section className={own.letter} aria-labelledby="signed">
						{agent?.avatar && (
							<Avatar
								avatarKey={agent.avatar as AvatarKey}
								name={name}
								size={48}
							/>
						)}
						<div>
							<p id="signed" className={styles.muted}>
								{t("marketingSigned", { name })}
							</p>
							<p>{plan.summary}</p>
						</div>
					</section>
					{proposed && (
						<section className={styles.section} aria-labelledby="allows">
							<h2 id="allows">{t("marketingAllowsTitle", { name })}</h2>
							<ul className={own.allows}>
								{plan.posts.length > 0 && (
									<li>
										{plan.posts.length === 1
											? t("marketingAllowsPostsOne")
											: t("marketingAllowsPosts", { n: plan.posts.length })}
									</li>
								)}
								{ads > 0 && (
									<li>
										{ads === 1
											? t("marketingAllowsAdsOne", {
													amount: cash(plan.budget.googleAds),
												})
											: t("marketingAllowsAds", {
													amount: cash(plan.budget.googleAds),
													n: ads,
												})}
									</li>
								)}
								<li>{t("marketingAllowsNothing")}</li>
								<li>{t("marketingAllowsEnd")}</li>
							</ul>
						</section>
					)}
					<div className={styles.section}>
						<h2 id="budget">{t("marketingBudget")}</h2>
						<p className={styles.muted}>
							{t(ads > 0 ? "marketingBudgetIn" : "marketingBudgetInPlain", {
								currency: currencyWords(plan.currency),
							})}
						</p>
						<section
							className={own.scroll}
							aria-labelledby="budget"
							// biome-ignore lint/a11y/noNoninteractiveTabindex: a scroll box must take focus to scroll by keyboard
							tabIndex={0}
						>
							<table
								className={own.budget}
								aria-label={t("marketingBudgetCaption")}
							>
								<thead>
									<tr>
										<th scope="col">{t("marketingColChannel")}</th>
										<th scope="col">{t("marketingColDates")}</th>
										<th scope="col" className={own.number}>
											{t("marketingColBudget")}
										</th>
									</tr>
								</thead>
								<tbody>
									{ads > 0 && (
										<>
											<tr>
												<td>
													<strong>{t("marketingGoogleAds")}</strong>
													<span className={own.sub}>
														{ads === 1
															? t("marketingCampaignOne")
															: t("marketingCampaigns", { n: ads })}
													</span>
												</td>
												<td>
													{shortRange(
														plan.campaigns.map((c) => c.startsOn).sort()[0] ??
															"",
														plan.campaigns
															.map((c) => c.endsOn)
															.sort()
															.at(-1) ?? "",
													)}
												</td>
												<td className={own.number}>
													{cash(plan.budget.googleAds)}
												</td>
											</tr>
											{plan.campaigns.map((campaign) => (
												<tr key={campaign.key} className={own.campaign}>
													<td>
														{campaign.name}
														<span className={own.sub}>{campaign.goal}</span>
													</td>
													<td>
														{shortRange(campaign.startsOn, campaign.endsOn)}
													</td>
													<td className={own.number}>
														{cash(campaign.budget)}
													</td>
												</tr>
											))}
										</>
									)}
									{channels.map((channel) => {
										const days = plan.posts
											.filter((post) => post.channel === channel)
											.map((post) => post.on)
											.sort();
										return (
											<tr key={channel}>
												<td>
													<strong>{channelName(channel)}</strong>
													<span className={own.sub}>
														{postsWord(days.length)}
													</span>
												</td>
												<td>{shortRange(days[0] ?? "", days.at(-1) ?? "")}</td>
												<td className={own.number}>{t("marketingNoCost")}</td>
											</tr>
										);
									})}
								</tbody>
								<tfoot>
									<tr>
										<th scope="row" colSpan={2}>
											{t("marketingTotal")}
										</th>
										<td className={own.number}>
											{t("marketingTotalAmount", {
												amount: cash(plan.budget.total),
												currency: plan.currency,
											})}
										</td>
									</tr>
								</tfoot>
							</table>
						</section>
					</div>
					{(proposed || live || written.length > 0) &&
						plan.posts.length > 0 && (
							<section className={styles.section} aria-labelledby="calendar">
								<h2 id="calendar">{t("marketingCalendar")}</h2>
								<p className={styles.muted}>
									{t(
										plan.posts.length === 1
											? "marketingCalendarLeadOne"
											: "marketingCalendarLead",
										{
											n: plan.posts.length,
											channels: listed(channels.map(channelName)),
											name,
										},
									)}
								</p>
								{!proposed && counts !== "" && (
									<p className={styles.muted}>{counts}</p>
								)}
								{!proposed && (
									<p className={styles.muted}>{t("planPostsSentMeans")}</p>
								)}
								{weeks(plan.startsOn, plan.endsOn, plan.posts).map((week) => (
									<Fragment key={week.n}>
										<h3 id={`week-${week.n}`} className={own.week}>
											{t("marketingWeek", { n: week.n, range: week.range })}
										</h3>
										<ul
											aria-labelledby={`week-${week.n}`}
											className={own.posts}
										>
											{week.slots.map((post) => (
												<li key={post.key}>
													<time dateTime={post.on}>{shortDay(post.on)}</time>
													<span className={own.channel}>
														{channelName(post.channel)}
													</span>
													<span className={own.topic}>{post.topic}</span>
													{!proposed && (
														<SlotPost
															written={written}
															slot={post.key}
															instant={instant}
														/>
													)}
												</li>
											))}
										</ul>
									</Fragment>
								))}
							</section>
						)}
					{proposed && plan.measures.length > 0 && (
						<section className={styles.section}>
							<h2 id="measures">{t("marketingMeasures", { name })}</h2>
							<ul aria-labelledby="measures">
								{plan.measures.map((line) => (
									<li key={line}>{line}</li>
								))}
							</ul>
						</section>
					)}
					{proposed && (
						<section className={styles.section} aria-labelledby="whole">
							<h2 id="whole">{t("marketingWhole", { name })}</h2>
							<p className={styles.muted}>
								{t("marketingWholeHint", { name })}
							</p>
							{/* What the agent wrote, as text: React shows it as typed, never as markup. */}
							<pre className={own.text}>{plan.text}</pre>
						</section>
					)}
				</div>
			</div>
			{decide.refusal && (
				<p role="alert" className={styles.alert}>
					{decide.refusal}
				</p>
			)}
			{proposed && (
				<div className={own.bar}>
					<div className={styles.actions}>
						<Button
							kind="primary"
							busy={decide.busy}
							onClick={() =>
								decide.send({
									command: "marketing_plan_decide",
									body: { plan: plan.plan, decision: "approve" },
								})
							}
						>
							{t("marketingApprove")}
						</Button>
						<Button onClick={() => setAsking("back")}>
							{t("marketingSendBack")}
						</Button>
					</div>
					{what.length > 0 && (
						<p className={styles.muted}>
							{t("marketingBarLead", { name, what: what.join(" and ") })}
						</p>
					)}
				</div>
			)}
			{live && (
				<div className={own.bar}>
					<div className={styles.actions}>
						<Button onClick={() => setAsking("end")}>
							{t("marketingEnd")}
						</Button>
					</div>
					<p className={styles.muted}>{t("marketingEndHint", { name })}</p>
				</div>
			)}
			{asking === "back" && (
				<SendBack
					plan={plan.plan}
					name={name}
					onClose={() => setAsking(undefined)}
					onSent={() => {
						setAsking(undefined);
						again();
					}}
				/>
			)}
			{asking === "end" && (
				<EndPlan
					plan={plan}
					name={name}
					now={now}
					instant={instant}
					onClose={() => setAsking(undefined)}
					onSent={() => {
						setAsking(undefined);
						again();
					}}
				/>
			)}
		</div>
	);
}

/**
 * One fact of the side column. On a phone only the plan's own figures show, ahead of the
 * summary; the `extra` ones, which say where the plan came from and what became of it, are for
 * a wide screen, as the phone's mockup draws it.
 */
function Fact({
	term,
	extra,
	children,
}: {
	term: string;
	extra?: boolean;
	children: ReactNode;
}) {
	return (
		<div className={extra ? own.extra : own.fact}>
			<dt>{term}</dt>
			<dd>{children}</dd>
		</div>
	);
}

/** The title, where the plan stands, and what the owner did to it, in their own words. */
function Header({
	plan,
	name,
	now,
}: {
	plan: Plan;
	name: string;
	now: string;
}) {
	const started = plan.startsOn <= now;
	const state = {
		proposed: t("marketingStateProposed"),
		returned: t("marketingStateReturned"),
		approved: started
			? t("marketingStateApproved")
			: t("marketingStateApprovedStarts", { start: longDay(plan.startsOn) }),
		active: t("marketingStateActive"),
		ended: t("marketingStateEnded"),
	}[plan.state];
	const lead =
		plan.state === "proposed"
			? t("marketingLeadProposed", { id: plan.plan })
			: plan.state === "active"
				? t("marketingLeadActive", {
						id: plan.plan,
						day: dayOf(plan.startsOn, plan.endsOn, now),
						days: length(plan.startsOn, plan.endsOn),
						end: longDay(plan.endsOn),
					})
				: t("marketingLead", { id: plan.plan });
	const decided = plan.decided;
	const words =
		decided?.decision === "approved" ? decided.note : decided?.reason;
	return (
		<div className={styles.section}>
			<h1 className={styles.title}>{plan.title}</h1>
			<p className={own.state} data-state={plan.state}>
				{state}
			</p>
			<p className={styles.muted}>{lead}</p>
			{plan.state !== "proposed" && decided && !plan.ended && (
				<div className={own.decision}>
					<p>
						{t(
							decided.decision === "approved"
								? "marketingYouApproved"
								: "marketingYouSentBack",
							{ at: when(decided.at) },
						)}
					</p>
					{words && <blockquote>{`“${words}”`}</blockquote>}
					{decided.decision === "returned" && (
						<p className={styles.muted}>{t("marketingWriting", { name })}</p>
					)}
				</div>
			)}
			{plan.ended && (
				<div className={own.decision}>
					{plan.ended.why === "replaced" && plan.ended.replacedBy ? (
						<p>
							{t("marketingEndedReplacedBy", { day: longDay(plan.ended.at) })}
							<Link to={`/marketing/plans/${plan.ended.replacedBy}`}>
								{plan.ended.replacedBy}
							</Link>
							{t("marketingEndedReplacedByTail")}
						</p>
					) : (
						<p>
							{plan.ended.why === "by_owner"
								? t("marketingYouEnded", { at: when(plan.ended.at) })
								: t(
										plan.ended.why === "expired"
											? "marketingEndedExpired"
											: "marketingEndedReplaced",
										{ day: longDay(plan.ended.at) },
									)}
						</p>
					)}
					{plan.ended.why === "by_owner" && plan.ended.note && (
						<blockquote>{`“${plan.ended.note}”`}</blockquote>
					)}
					{decided?.decision === "approved" && (
						<p className={styles.muted}>
							{t("marketingApprovedItOn", {
								day: longDay(decided.at),
								name,
							})}
						</p>
					)}
				</div>
			)}
		</div>
	);
}

/** The words of 600 characters at most, with how many are used. */
function Words({
	id,
	label,
	value,
	onChange,
	required,
}: {
	id: string;
	label: string;
	value: string;
	onChange: (value: string) => void;
	required?: boolean;
}) {
	const used = counted(value);
	return (
		<>
			<TextArea
				id={id}
				label={label}
				hint={t("marketingBackHint")}
				value={value}
				onChange={onChange}
				{...(required ? { required } : {})}
			/>
			<p
				className={used > MAX_NOTE ? own.over : styles.muted}
				aria-live="polite"
			>
				{t("marketingBackCount", { n: used })}
			</p>
		</>
	);
}

/** Where a slot's post stands: the state, the post's words as typed, and an earlier failure. */
function SlotPost({
	written,
	slot,
	instant,
}: {
	written: WrittenPost[];
	slot: string;
	instant: Date;
}) {
	const { shown, standing, failedAt } = slotStanding(written, slot);
	const detail = standingDetail(standing, shown, instant);
	return (
		<span className={own.written}>
			<span>
				<strong>{standingWord(standing)}</strong>
				{detail !== "" && ` ${detail}`}
			</span>
			{/* What the agent wrote: text, never markup. */}
			{shown && <span className={own.slotText}>{shown.text}</span>}
			{failedAt && (
				<span className={own.sub}>
					{t("planSlotFailedBefore", { time: clock(new Date(failedAt)) })}
				</span>
			)}
		</span>
	);
}

/** "Send back": why, in the owner's words, which the agent reads as such. */
function SendBack({
	plan,
	name,
	onClose,
	onSent,
}: {
	plan: string;
	name: string;
	onClose: () => void;
	onSent: () => void;
}) {
	const [why, setWhy] = useState("");
	const { busy, refusal, send } = useCommand(onSent);
	const used = counted(why);
	return (
		<Dialog
			open
			title={t("marketingBackTitle", { name })}
			onClose={onClose}
			actions={
				<Button
					kind="primary"
					busy={busy}
					disabled={used === 0 || used > MAX_NOTE}
					onClick={() =>
						send({
							command: "marketing_plan_decide",
							body: { plan, decision: "return", note: why.trim() },
						})
					}
				>
					{t("marketingBackSend")}
				</Button>
			}
		>
			<p>{t("marketingBackLead", { name })}</p>
			<Words
				id="plan-back-why"
				label={t("marketingBackLabel")}
				value={why}
				onChange={setWhy}
				required
			/>
			{refusal && <p role="alert">{refusal}</p>}
		</Dialog>
	);
}

/** "End the plan": what ending does first, then the optional note. */
function EndPlan({
	plan,
	name,
	now,
	instant,
	onClose,
	onSent,
}: {
	plan: Plan;
	name: string;
	now: string;
	/** This moment, for the posts' own times. */
	instant: Date;
	onClose: () => void;
	onSent: () => void;
}) {
	const [note, setNote] = useState("");
	const { busy, refusal, send } = useCommand(onSent);
	const used = counted(note);
	const line =
		plan.startsOn > now
			? t("marketingEndLineLater", { title: plan.title, id: plan.plan })
			: plan.state === "active"
				? t("marketingEndLine", {
						title: plan.title,
						id: plan.plan,
						day: dayOf(plan.startsOn, plan.endsOn, now),
						days: length(plan.startsOn, plan.endsOn),
					})
				: t("marketingEndLinePlain", { title: plan.title, id: plan.plan });
	return (
		<Dialog
			open
			title={t("marketingEndTitle")}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("marketingEndKeep")}</Button>
					<Button
						kind="primary"
						busy={busy}
						disabled={used > MAX_NOTE}
						onClick={() =>
							send({
								command: "marketing_plan_end",
								body: {
									plan: plan.plan,
									...(used > 0 ? { note: note.trim() } : {}),
								},
							})
						}
					>
						{t("marketingEnd")}
					</Button>
				</>
			}
		>
			<p>{line}</p>
			{endingPosts(plan.writtenPosts ?? [], instant).map((words) => (
				<p key={words}>{words}</p>
			))}
			<p>{t("marketingEndEffect", { name })}</p>
			<Words
				id="plan-end-note"
				label={t("marketingEndNote", { name })}
				value={note}
				onChange={setNote}
			/>
			{refusal && <p role="alert">{refusal}</p>}
		</Dialog>
	);
}
