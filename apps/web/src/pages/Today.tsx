import mark from "@catervas/brand/assets/logo-mark-1254.png";
import { Button, RoleTag, uiStrings } from "@catervas/ui";
import { type FormEvent, type ReactNode, useState } from "react";
import { Link, useNavigate } from "react-router";
import { agentLine } from "../app/agentLine.ts";
import { useCommand } from "../app/command.ts";
import { useConnection } from "../app/connection.tsx";
import { type ProductPlanState, projectName } from "../app/productPlan.ts";
import { daemonSaid, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { codeOf } from "../app/words.ts";
import { AgentPicture } from "../components/AgentPicture.tsx";
import {
	AttachButton,
	AttachedChips,
	useAttachments,
} from "../components/Attach.tsx";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import { type AdsAsk, AdsRow } from "./AdsRow.tsx";
import { type Allowances, useAllowances } from "./allowances.tsx";
import {
	type Catervafication,
	CatervaficationCard,
} from "./CatervaficationCard.tsx";
import { ChannelPreview } from "./Channel.tsx";
import {
	DocumentChanges,
	type DocumentChangesCard,
} from "./DocumentChanges.tsx";
import { DataPipeline, type PipelineAsk } from "./dialogs/DataPipeline.tsx";
import { isScript, ScriptWarning } from "./dialogs/ScriptWarning.tsx";
import {
	ToolApproval,
	type ToolAsk,
	visibly,
} from "./dialogs/ToolApproval.tsx";
import { channelName, longRange, money } from "./marketing.ts";
import { type OrderWaiting, PurchaseOrderRow } from "./OrderRow.tsx";
import type { OrderAsk } from "./orders.ts";
import {
	GoingOut,
	type GoingOutPost,
	PostCard,
	type PostMedia,
} from "./PostGoingOut.tsx";
import { RenewalsSection } from "./Renewals.tsx";
import { SellerMailSections } from "./SellerMail.tsx";
import { SuggestedConnections } from "./SuggestedConnections.tsx";
import { type Agent, roleName, type Team } from "./setup/TeamSetup.tsx";
import type { RoleKit } from "./Team.tsx";
import styles from "./Today.module.css";
import {
	GROUP_ORDER,
	groupOf,
	MERGE_SHOWN,
	type WaitingGroup,
} from "./waitingGroups.ts";

type Activity = { agentId: string; state: string; line: string };
type Kind =
	| "approval"
	| "acceptance"
	| "question"
	| "help"
	| "integration"
	| "preview_missing"
	| "designer_needs_sandbox"
	| "designer_needs_browser";
/** What a marketing plan waiting on the owner says of itself (`waiting.list`). */
type PlanAsk = {
	plan: string;
	summary: string;
	total: string;
	currency: string;
	startsOn: string;
	endsOn: string;
};
/** What a post outside the plan waiting on the owner says of itself (`waiting.list`). */
type PostAsk = {
	post: number;
	channel: string;
	text: string;
	media: PostMedia[];
	at: string;
};
type Waiting = {
	/** Absent on a folder change waiting to be added, which no task holds. */
	taskId?: string | undefined;
	/** The folder change an `integration` row waits to add, when it is about one. */
	folderChange?: number | undefined;
	kind:
		| Kind
		| "tool_approval"
		| "marketing_plan"
		| "social_post"
		| "purchase_order"
		| "data_pipeline"
		| AdsAsk["kind"];
	agentId: string | null;
	title: string;
	line: string;
} & Partial<ToolAsk> &
	Partial<Omit<AdsAsk, "kind" | "taskId" | "agentId" | "plan">> &
	Partial<PlanAsk> &
	Partial<PostAsk> &
	Partial<OrderAsk> &
	Partial<PipelineAsk>;
type Moved = { at: string; by: string; line: string };
type Check = { passed: boolean };

/** Each kind's title, button word and page (the step 08 plan's routes); `line` in place of the daemon's. */
const KINDS: Record<
	Kind,
	{
		title: keyof typeof en;
		word: keyof typeof en;
		page: string;
		line?: keyof typeof en;
	}
> = {
	approval: { title: "waitingApproval", word: "waitingReview", page: "plan" },
	acceptance: {
		title: "waitingAcceptance",
		word: "waitingReview",
		page: "accept",
	},
	question: {
		title: "waitingQuestion",
		word: "waitingAnswer",
		page: "questions",
	},
	help: { title: "waitingHelp", word: "waitingHelpAnswer", page: "help" },
	integration: {
		title: "waitingIntegration",
		word: "waitingAdd",
		page: "accept",
	},
	preview_missing: {
		title: "waitingPreviewMissing",
		word: "waitingOpenSettings",
		page: "/settings/your-app",
		line: "waitingPreviewMissingLine",
	},
	designer_needs_sandbox: {
		title: "waitingNeedsSandbox",
		word: "waitingOpenTeam",
		page: "/team",
		line: "waitingNeedsSandboxLine",
	},
	// The Designer's own page on the Team page, where its Playwright is turned on.
	designer_needs_browser: {
		title: "waitingNeedsBrowser",
		word: "waitingOpenTeam",
		page: "/team/{agent}",
		line: "waitingNeedsBrowserLine",
	},
};

const DAY_MS = 86_400_000;

/** Whether a row is about a marketing plan's ads and their budget (step 08g). */
const isAds = (kind: Waiting["kind"]): kind is AdsAsk["kind"] =>
	kind === "marketing_budget" ||
	kind === "marketing_ads_running" ||
	kind === "marketing_spend_unread";

/** The home page: the team, the request box, what waits on the human, and what moved. */
export function Today() {
	const { data: team } = useQuery<{
		team: Team;
		kits?: RoleKit[];
		hiddenSuggestions?: { agent: string; server: string }[];
		requestedConnections?: { agent: string; server: string; why: string }[];
	}>("team.get", {});
	// What agents made on other services, for a call that waits past its allowance.
	const allowances = useAllowances();
	const { data: activity } = useQuery<{ activity: Activity[] }>(
		"team.activity",
		{},
	);
	const { data: listed } = useQuery<{ waiting: Waiting[] }>("waiting.list", {});
	// A row of a kind this page does not know is left out, as is its count: a newer daemon may
	// have more kinds than this page.
	const waiting = listed && {
		waiting: listed.waiting.filter(
			(item) =>
				item.kind === "tool_approval" ||
				item.kind === "marketing_plan" ||
				item.kind === "social_post" ||
				item.kind === "purchase_order" ||
				item.kind === "data_pipeline" ||
				isAds(item.kind) ||
				item.kind in KINDS,
		),
	};
	// Once, so the query's key stays the same between renders.
	const [since] = useState(() => new Date(Date.now() - DAY_MS).toISOString());
	const today = new Date().toISOString().slice(0, 10);
	const { data: moved } = useQuery<{ moved: Moved[] }>("moved.since", {
		since,
	});
	const { data: posts, again: askPostsAgain } = useQuery<{
		posts: GoingOutPost[];
	}>("social_posts.list", {});
	// The product plan's changes, one card for each review and agent that proposed them.
	const { data: planChanges, again: askPlanChangesAgain } = useQuery<{
		cards: DocumentChangesCard[];
	}>("folder_doc_proposals.list", {});
	const cards = planChanges?.cards ?? [];
	// A new project's product plan: while it is interviewed, the band says so; once both documents
	// are written, the plan's acceptance row is shown as the plan itself.
	const { data: plan } = useQuery<ProductPlanState>("product_plan.state", {});
	const interviewing = !!plan?.newProject && !plan.approved;
	// "Later" leaves a quiet card here until the owner starts the documenting work.
	const { data: documenting, again: askDocumentingAgain } =
		useQuery<Catervafication>("catervafication.get", {});
	// While the provider refuses the key the team is paused, and one row says so first.
	const { data: account } = useQuery<{ keyRefused?: boolean }>(
		"account.status",
		{},
	);
	const keyRefused = account?.keyRefused === true;
	const agents = team?.team.agents ?? [];
	const agent = (id: string | null) => agents.find((a) => a.id === id);
	const pm = agents.find(
		(a) => a.role === "product_manager" && a.status !== "retired",
	);
	const waitsForPlan = t("todayWaitsForPlan", {
		project: projectName(plan?.newProject?.name ?? ""),
	});
	// One row, whatever its kind.
	const row = (item: Waiting): ReactNode =>
		item.kind === "tool_approval" ? (
			<ToolApprovalRow
				key={`${item.kind}-${item.approval}`}
				item={item}
				agent={agent(item.agentId)}
				allowances={allowances}
				kits={team?.kits ?? []}
			/>
		) : item.kind === "marketing_plan" ? (
			<MarketingPlanRow
				key={`${item.kind}-${item.plan}`}
				item={item}
				agent={agent(item.agentId)}
			/>
		) : item.kind === "social_post" ? (
			<PostRequestRow
				key={`${item.kind}-${item.post}`}
				item={item}
				agent={agent(item.agentId)}
				now={new Date()}
			/>
		) : isAds(item.kind) ? (
			<AdsRow
				key={`${item.kind}-${item.plan}`}
				item={item as AdsAsk}
				agent={agent(item.agentId)}
			/>
		) : item.kind === "purchase_order" ? (
			<PurchaseOrderRow
				key={`${item.kind}-${item.order}`}
				item={item as OrderWaiting}
				agent={agent(item.agentId)}
				now={new Date()}
			/>
		) : item.kind === "data_pipeline" ? (
			<PipelineRow
				key={`${item.kind}-${item.pipeline}`}
				item={item}
				agent={agent(item.agentId)}
			/>
		) : (
			<WaitingRow
				key={`${item.kind}-${item.taskId ?? `folder-${item.folderChange}`}`}
				item={item}
				agent={agent(item.agentId)}
				developer={
					agents.find(
						(a) => a.role === "software_developer" && a.status !== "retired",
					)?.displayName ?? ""
				}
			/>
		);
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("today")}</h1>
			<div className={styles.band}>
				{interviewing && plan.newProject ? (
					<p>
						{pm ? (
							<Link to={`/channel/${pm.id}`}>{waitsForPlan}</Link>
						) : (
							waitsForPlan
						)}
					</p>
				) : null}
				<ul className={styles.agents} aria-label={t("teamBand")}>
					{activity?.activity.map((one) => (
						<AgentEntry
							key={one.agentId}
							one={one}
							agent={agent(one.agentId)}
						/>
					))}
				</ul>
			</div>
			<RequestBox
				pmName={pm?.displayName ?? uiStrings.roleName.product_manager}
				soloName={
					team?.team.policy.solo
						? team.team.agents.find((a) => a.status !== "retired")?.displayName
						: undefined
				}
			/>
			{documenting?.offered && (
				<CatervaficationCard again={askDocumentingAgain} />
			)}
			{waiting && (
				<section className={styles.section} aria-labelledby="waiting-heading">
					<h2 id="waiting-heading">
						{t("waitingTitle", {
							count: String(
								waiting.waiting.length + cards.length + (keyRefused ? 1 : 0),
							),
						})}
					</h2>
					{waiting.waiting.length === 0 && cards.length === 0 && !keyRefused ? (
						<p className={styles.muted}>{t("waitingNone")}</p>
					) : (
						GROUP_ORDER.map((group) => {
							const rows: ReactNode[] = [];
							if (group === "set_up" && keyRefused) {
								rows.push(<KeyRefusedRow key="key-refused" />);
							}
							for (const item of waiting.waiting) {
								if (groupOf(item.kind) === group) {
									rows.push(row(item));
								}
							}
							if (group === "answer" && planChanges) {
								for (const card of cards) {
									const one = agent(card.agentId);
									rows.push(
										<li key={`plan-${card.sprintId}-${card.agentId}`}>
											<DocumentChanges
												card={card}
												name={one?.displayName ?? card.agentId}
												agent={one}
												again={askPlanChangesAgain}
											/>
										</li>,
									);
								}
							}
							if (rows.length === 0) return null;
							return (
								<WaitingGroupSection key={group} group={group}>
									{rows}
								</WaitingGroupSection>
							);
						})
					)}
				</section>
			)}
			<SellerMailSections
				agents={agents}
				pm={pm?.displayName ?? uiStrings.roleName.product_manager}
			/>
			<RenewalsSection
				agent={
					agents.find(
						(a) =>
							a.role === "procurement_specialist" && a.status !== "retired",
					)?.displayName ?? uiStrings.roleName.procurement_specialist
				}
				pm={pm?.displayName ?? uiStrings.roleName.product_manager}
				now={new Date()}
			/>
			{posts && (
				<GoingOut
					posts={posts.posts}
					agents={agents}
					now={new Date()}
					again={askPostsAgain}
				/>
			)}
			{moved && (
				<section className={styles.section} aria-labelledby="moved-heading">
					<h2 id="moved-heading">{t("movedTitle")}</h2>
					{moved.moved.length === 0 ? (
						<p className={styles.muted}>{t("movedNone")}</p>
					) : (
						<MovedList moved={moved.moved} agents={agents} today={today} />
					)}
				</section>
			)}
			{team && (
				<SuggestedConnections
					team={team.team}
					kits={team.kits ?? []}
					hidden={team.hiddenSuggestions ?? []}
					requested={team.requestedConnections ?? []}
				/>
			)}
			<ChannelPreview agents={agents} />
		</div>
	);
}

const GROUP_NAME: Record<WaitingGroup, keyof typeof en> = {
	set_up: "waitingGroupSetUp",
	answer: "waitingGroupAnswer",
	merge: "waitingGroupMerge",
};

/** One group of "Waiting on you": its heading with the count, and its rows; the merge group
 * shows three and folds the rest behind "Show all". */
function WaitingGroupSection({
	group,
	children,
}: {
	group: WaitingGroup;
	children: ReactNode[];
}) {
	const [open, setOpen] = useState(false);
	const name = t(GROUP_NAME[group]);
	const folds = group === "merge" && children.length > MERGE_SHOWN;
	const shown = folds && !open ? children.slice(0, MERGE_SHOWN) : children;
	const listId = `waiting-group-${group}`;
	return (
		<section aria-labelledby={`${listId}-heading`}>
			<h3 id={`${listId}-heading`} className={styles.groupTitle}>
				{t("waitingGroupCount", {
					group: name,
					count: String(children.length),
				})}
			</h3>
			<ul id={listId} className={styles.rows} aria-label={name}>
				{shown}
			</ul>
			{folds && (
				<div className={styles.movedMore}>
					<Button
						kind="quiet"
						expanded={open}
						controls={listId}
						onClick={() => setOpen(!open)}
					>
						{open
							? t("suggestionsShowFewer")
							: t("suggestionsShowAll", { count: children.length })}
					</Button>
				</div>
			)}
		</section>
	);
}

/** How many moves show at first, and how many each "Show more" adds. */
const MOVED_STEP = 3;

/** What moved, newest first: three cards, then "Show more" adds three; "Show fewer" goes back. */
function MovedList({
	moved,
	agents,
	today,
}: {
	moved: Moved[];
	agents: Agent[];
	today: string;
}) {
	const [shown, setShown] = useState(MOVED_STEP);
	// The daemon answers oldest first, as a log reads; the page turns it round.
	const newest = [...moved].reverse();
	const hidden = newest.length - shown;
	return (
		<>
			<ul className={styles.moved} aria-label={t("movedTitle")}>
				{newest.slice(0, shown).map((one) => (
					<MovedCard
						key={`${one.at}-${one.by}-${one.line}`}
						one={one}
						agent={agents.find((a) => a.id === one.by)}
						today={today}
					/>
				))}
			</ul>
			{newest.length > MOVED_STEP && (
				<div className={styles.movedMore}>
					<Button
						onClick={() =>
							setShown(hidden > 0 ? shown + MOVED_STEP : MOVED_STEP)
						}
					>
						{hidden > 0 ? t("movedMore", { n: hidden }) : t("movedFewer")}
					</Button>
				</div>
			)}
		</>
	);
}

/** One move: who made it (picture, name, role tag), when, and the sentence. */
function MovedCard({
	one,
	agent,
	today,
}: {
	one: Moved;
	agent: Agent | undefined;
	today: string;
}) {
	// Times are HH:MM UTC, as the team's own lines say them; the list holds a day, so an
	// earlier date is yesterday's.
	const hhmm = one.at.slice(11, 16);
	const time =
		one.at.slice(0, 10) === today ? hhmm : t("movedYesterday", { time: hhmm });
	let picture: ReactNode = null;
	let name: string;
	if (one.by === "human") {
		name = t("movedYou");
		picture = (
			<span className={styles.you} aria-hidden="true">
				<svg
					width="24"
					height="24"
					viewBox="0 0 22 22"
					fill="none"
					stroke="currentColor"
					strokeWidth="1.75"
					aria-hidden="true"
				>
					<circle cx="11" cy="8" r="3.5" />
					<path d="M4 19c1-3.6 3.8-5.5 7-5.5s6 1.9 7 5.5" />
				</svg>
			</span>
		);
	} else if (one.by === "catervas") {
		name = t("brand");
		picture = <img src={mark} alt="" width={48} height={48} />;
	} else {
		name = agent?.displayName ?? one.by;
		picture = <AgentPicture agent={agent} size={48} />;
	}
	return (
		<li>
			{picture}
			<div className={styles.movedText}>
				<span className={styles.movedHead}>
					<strong>{name}</strong>
					{agent && one.by !== "human" && one.by !== "catervas" && (
						<RoleTag role={agent.role} />
					)}
					<time dateTime={one.at}>{time}</time>
				</span>
				<span>{one.line}</span>
			</div>
		</li>
	);
}

function AgentEntry({
	one,
	agent,
}: {
	one: Activity;
	agent: Agent | undefined;
}) {
	const name = agent?.displayName ?? one.agentId;
	const shown = visibly(agentLine(one));
	return (
		<li className={styles.agent}>
			<AgentPicture agent={agent} size={48} />
			<span className={styles.who}>
				<strong>{name}</strong> {agent && <RoleTag role={agent.role} />}
			</span>
			<span className={styles.line} title={shown}>
				{shown}
			</span>
		</li>
	);
}

/** The request box: the person's words, sent as `request.file`, and the request's page after. */
/** The request box; with one agent alone (ADR 0071) it names that agent, who takes the request itself. */
function RequestBox({
	pmName,
	soloName,
}: {
	pmName: string;
	soloName?: string | undefined;
}) {
	const { client } = useConnection();
	const navigate = useNavigate();
	const [text, setText] = useState("");
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const attach = useAttachments();
	const send = async (e: FormEvent) => {
		e.preventDefault();
		if (!client || attach.busy) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const filed = (await client.call("request.file", {
				text,
				...(attach.paths.length ? { attachments: attach.paths } : {}),
			})) as {
				taskId: string;
			};
			attach.clear();
			navigate(`/requests/${filed.taskId}`);
		} catch (error) {
			// A coded refusal (the length one) is worded by its code; the store's own sentences as they are.
			setRefusal(
				codeOf(error) ? saidAll(error) : daemonSaid(error, "refuseCommand"),
			);
			setBusy(false);
		}
	};
	return (
		<form className={styles.request} onSubmit={send}>
			<label htmlFor="request" className={styles.label}>
				{soloName
					? t("requestLabelSolo", { name: soloName })
					: t("requestLabel")}
			</label>
			<AttachedChips items={attach.items} onRemove={attach.remove} />
			<div className={styles.prompt}>
				<span aria-hidden="true">&gt;</span>
				<textarea
					id="request"
					rows={3}
					value={text}
					aria-describedby="request-hint"
					onChange={(e) => setText(e.target.value)}
				/>
				{/* The page's only idle motion; stopped under reduced motion. */}
				{text === "" && <span className={styles.cursor} aria-hidden="true" />}
			</div>
			<p id="request-hint" className={styles.muted}>
				{soloName
					? t("requestHintSolo", { name: soloName })
					: t("requestHint", { name: pmName })}
			</p>
			{(refusal ?? attach.refusal) && (
				<p role="alert" className={styles.alert}>
					{refusal ?? attach.refusal}
				</p>
			)}
			<div className={styles.sendRow}>
				{/* Without a connection the request cannot be filed: the button waits for one. */}
				<Button
					kind="primary"
					type="submit"
					busy={busy}
					disabled={!client || attach.busy}
				>
					{soloName
						? t("requestSendSolo", { name: soloName })
						: t("requestSend")}
				</Button>
				<AttachButton onPick={(file, kind) => void attach.add(file, kind)} />
			</div>
		</form>
	);
}

function WaitingRow({
	item,
	agent,
	developer,
}: {
	item: Waiting;
	agent: Agent | undefined;
	developer: string;
}) {
	const kind = KINDS[item.kind as Kind];
	const name = agent?.displayName ?? item.agentId ?? "";
	const [notYet, setNotYet] = useState(false);
	// A merge checked and not yet seen answers with no events (SPEC 4.2); the list refetches on any event.
	const { busy, refusal, send } = useCommand((done) =>
		setNotYet(done.events.length === 0),
	);
	const titleId = `waiting-${item.kind}-${item.taskId ?? `folder-${item.folderChange}`}`;
	const title = t(kind.title, { title: item.title, agent: name });
	const mergeCheck = item.kind === "integration" && item.taskId !== undefined;
	return (
		<li className={styles.row}>
			<AgentPicture agent={agent} />
			<div className={styles.rowText}>
				{mergeCheck ? (
					<Link to={`/tasks/${item.taskId}/accept`}>
						<strong id={titleId}>{title}</strong>
					</Link>
				) : (
					<strong id={titleId}>{title}</strong>
				)}
				{(item.kind === "question" || item.kind === "help") && (
					<span className={styles.muted}>
						{t("waitingAbout", { title: item.title })}
					</span>
				)}
				<span
					className={
						item.kind === "question" || item.kind === "help"
							? styles.clamp3
							: undefined
					}
				>
					{kind.line ? t(kind.line, { designer: name, developer }) : item.line}
				</span>
				{item.kind === "acceptance" && item.taskId && (
					<ChecksPassed taskId={item.taskId} />
				)}
				{refusal ? (
					<p role="alert" className={styles.alert}>
						{refusal}
					</p>
				) : (
					notYet && (
						<p role="status" className={styles.muted}>
							{t("waitingNotMergedYet")}
						</p>
					)
				)}
			</div>
			{mergeCheck ? (
				<div className={styles.decide}>
					<Button
						busy={busy}
						aria-describedby={titleId}
						onClick={() => {
							setNotYet(false);
							send({
								command: "task_integrate",
								body: { taskId: item.taskId },
							});
						}}
					>
						{t("waitingMerged")}
					</Button>
				</div>
			) : item.folderChange !== undefined ? (
				<div className={styles.decide}>
					<Button
						busy={busy}
						aria-describedby={titleId}
						onClick={() =>
							send({
								command: "folder_change_integrate",
								body: { change: item.folderChange },
							})
						}
					>
						{t(kind.word)}
					</Button>
				</div>
			) : (
				<Link
					className={styles.action}
					to={
						kind.page.startsWith("/")
							? kind.page.replace("{agent}", item.agentId ?? "")
							: `/tasks/${item.taskId}/${kind.page}`
					}
					aria-describedby={titleId}
				>
					{t(kind.word)}
				</Link>
			)}
		</li>
	);
}

/** A marketing plan waiting for the owner: its summary, its budget and its dates; "Review" opens its page. */
function MarketingPlanRow({
	item,
	agent,
}: {
	item: Waiting;
	agent: Agent | undefined;
}) {
	const titleId = `waiting-plan-${item.plan}`;
	return (
		<li className={styles.row}>
			<AgentPicture agent={agent} />
			<div className={styles.rowText}>
				<strong id={titleId}>
					{t("waitingMarketingPlan", { title: item.title })}
				</strong>
				{/* Text, whatever the agent wrote; two lines here, all of it on the plan's page. */}
				<span className={styles.clamp}>{item.summary}</span>
				<span className={styles.figures}>
					<span className={styles.amount}>
						{money(item.total ?? "0.00", item.currency ?? "USD")}
					</span>
					<span>{item.currency}</span>
					<span>{longRange(item.startsOn ?? "", item.endsOn ?? "")}</span>
				</span>
			</div>
			<Link
				className={styles.action}
				to={`/marketing/plans/${item.plan}`}
				aria-describedby={titleId}
			>
				{t("waitingReview")}
			</Link>
		</li>
	);
}

/** A post outside the plan: its words and pictures, and the owner's "Post it" or "Don't post". */
function PostRequestRow({
	item,
	agent,
	now,
}: {
	item: Waiting;
	agent: Agent | undefined;
	now: Date;
}) {
	const name = agent?.displayName ?? item.agentId ?? "";
	const titleId = `waiting-post-${item.post}`;
	const { busy, refusal, send } = useCommand(() => {});
	const decide = (decision: "post" | "dont_post") =>
		send({
			command: "social_post_decide",
			body: { post: item.post, decision },
		});
	return (
		<li className={styles.row}>
			<AgentPicture agent={agent} />
			<div className={styles.rowText}>
				<strong id={titleId}>
					{t("waitingPost", {
						name,
						network: channelName(item.channel ?? ""),
					})}
				</strong>
				<span>{t("postAsksFirst", { name })}</span>
				<PostCard
					post={{
						post: item.post ?? 0,
						channel: item.channel ?? "",
						text: item.text ?? "",
						media: item.media ?? [],
						at: item.at ?? "",
					}}
					now={now}
				/>
				<span>{t("postIfYouAllow")}</span>
				{refusal && (
					<p role="alert" className={styles.alert}>
						{refusal}
					</p>
				)}
			</div>
			<div className={styles.decide}>
				<Button kind="primary" busy={busy} onClick={() => decide("post")}>
					{t("postItYes")}
				</Button>
				<Button busy={busy} onClick={() => decide("dont_post")}>
					{t("postItNo")}
				</Button>
			</div>
		</li>
	);
}

/**
 * A data source the Procurement Specialist asked for and the Product Manager passed on: why it
 * comes to the owner in Catervas's words, then everything others wrote, each in a frame that says
 * whose words they are, and "Approve" or "Decline", each of which asks for a note (spec 6.10).
 */
function PipelineRow({
	item,
	agent,
}: {
	item: Waiting;
	agent: Agent | undefined;
}) {
	const name = agent?.displayName ?? item.agentId ?? "";
	const source = visibly(item.name ?? "");
	const host = item.host ?? "";
	const url = item.url ?? "";
	const titleId = `waiting-pipeline-${item.pipeline}`;
	const [dialog, setDialog] = useState<"approve" | "decline">();
	const [forTask, forTaskEnd] = t("pipelineTask", {
		task: "{task}",
		name,
	}).split("{task}");
	const ask: PipelineAsk = {
		pipeline: item.pipeline ?? 0,
		name: item.name ?? "",
		what: item.what ?? "",
		url,
		host,
		why: item.why ?? "",
		cost: item.cost ?? "unknown",
		needsAccount: item.needsAccount ?? false,
		sendsProjectData: item.sendsProjectData ?? false,
		...(item.reason ? { reason: item.reason } : {}),
		requestText: item.requestText ?? "",
	};
	const ids = (part: string) => `${titleId}-${part}`;
	return (
		<li className={styles.row}>
			<AgentPicture agent={agent} />
			<div className={styles.rowText}>
				<strong id={titleId}>{t("pipelineLine", { name, source })}</strong>
				<span>
					{forTask}
					<Link to={`/tasks/${item.taskId}`}>
						{item.taskId} {item.title}
					</Link>
					{forTaskEnd}
				</span>
				<span className={styles.muted} id={ids("why-you")}>
					{t("pipelineWhyYou")}
				</span>
				<fieldset
					className={styles.pipelineWhy}
					aria-labelledby={ids("why-you")}
				>
					{ask.cost === "paid" && <p>{t("pipelinePaid")}</p>}
					{ask.cost === "unknown" && <p>{t("pipelineUnknown")}</p>}
					{ask.sendsProjectData && <p>{t("pipelineSendsData", { source })}</p>}
					{ask.reason !== undefined && (
						<>
							<span id={ids("asks")}>{t("pipelineAsks")}</span>
							<fieldset
								aria-labelledby={ids("asks")}
								data-trust="untrusted"
								className={styles.frame}
							>
								{visibly(ask.reason)}
							</fieldset>
						</>
					)}
				</fieldset>
				<span className={styles.muted} id={ids("what")}>
					{t("pipelineWhat", { name })}
				</span>
				<fieldset
					aria-labelledby={ids("what")}
					data-trust="untrusted"
					className={styles.frame}
				>
					{visibly(ask.what)}
				</fieldset>
				<span className={styles.muted} id={ids("why")}>
					{t("pipelineWhy", { name })}
				</span>
				<fieldset
					aria-labelledby={ids("why")}
					data-trust="untrusted"
					className={styles.frame}
				>
					{visibly(ask.why)}
				</fieldset>
				<span className={styles.muted}>{t("pipelineSource")}</span>
				<strong className={styles.host}>{host}</strong>
				{isScript(host) && <ScriptWarning className={styles.script} />}
				<code className={styles.address}>{visibly(url)}</code>
				{/^https?:\/\//.test(url) && (
					<a href={url} target="_blank" rel="noopener noreferrer">
						{t("pipelineOpen")}
					</a>
				)}
				{ask.needsAccount && <span>{t("pipelineAccount")}</span>}
				<span className={styles.muted}>{t("pipelineAsWritten", { name })}</span>
				<span className={styles.muted}>{t("pipelineNothingYet")}</span>
			</div>
			{/* The group names the source, so that each row's "Approve" is told from the others. */}
			<fieldset className={styles.decide} aria-labelledby={titleId}>
				<Button kind="primary" onClick={() => setDialog("approve")}>
					{t("pipelineApprove")}
				</Button>
				<Button onClick={() => setDialog("decline")}>
					{t("pipelineDecline")}
				</Button>
			</fieldset>
			{dialog && (
				<DataPipeline
					ask={ask}
					agent={name}
					approve={dialog === "approve"}
					onClose={() => setDialog(undefined)}
				/>
			)}
		</li>
	);
}

/** A connector call waiting to be allowed; "Review" opens its dialog. */
function ToolApprovalRow({
	item,
	agent,
	allowances,
	kits,
}: {
	item: Waiting;
	agent: Agent | undefined;
	allowances: Allowances | undefined;
	/** What Catervas offers each role, to name the service by its kit's title. */
	kits: RoleKit[];
}) {
	const [open, setOpen] = useState(false);
	const name = agent?.displayName ?? item.agentId ?? "";
	const titleId = `waiting-tool-${item.approval}`;
	// The agent's own role's kit, since two roles' kits may share a service name.
	const offered = kits.find((one) => one.role === agent?.role);
	const service = offered?.connectors.find((one) => one.name === item.server);
	const kit = offered &&
		service && { title: service.title, role: roleName(offered.role) };
	const ask: ToolAsk = {
		approval: item.approval ?? 0,
		server: item.server ?? "",
		tool: item.tool ?? "",
		input: item.input ?? "",
	};
	return (
		<li className={styles.row}>
			<AgentPicture agent={agent} />
			<div className={styles.rowText}>
				<strong id={titleId}>
					{t("waitingToolApproval", {
						agent: name,
						server: kit?.title ?? ask.server,
					})}
				</strong>
				<span>
					{t("waitingToolApprovalLine", {
						tool: ask.tool.replaceAll("_", " "),
						task: item.taskId ?? "",
						title: item.title,
						agent: name,
					})}
				</span>
			</div>
			<button
				type="button"
				className={styles.action}
				aria-describedby={titleId}
				onClick={() => setOpen(true)}
			>
				{t("waitingReview")}
			</button>
			{open && (
				<ToolApproval
					ask={ask}
					agent={name}
					agentId={item.agentId ?? ""}
					allowance={
						allowances && {
							row: allowances.rows.find(
								(row) =>
									row.agent === item.agentId &&
									row.server === ask.server &&
									row.tool === ask.tool,
							),
							period: allowances.period,
						}
					}
					kit={kit}
					taskId={item.taskId ?? ""}
					title={item.title}
					onClose={() => setOpen(false)}
				/>
			)}
		</li>
	);
}

/** The provider refused the AI account's key: the team waits until it is connected again. */
function KeyRefusedRow() {
	return (
		<li className={styles.row}>
			<div className={styles.rowText}>
				<strong id="waiting-key">{t("waitingKeyRefused")}</strong>
				<span>{t("waitingKeyRefusedLine")}</span>
			</div>
			<Link
				className={styles.action}
				to="/settings/account"
				aria-describedby="waiting-key"
			>
				{t("waitingKeyConnect")}
			</Link>
		</li>
	);
}

/** "All N of Catervas's checks passed.", shown only when every check of the task passed. */
function ChecksPassed({ taskId }: { taskId: string }) {
	const { data } = useQuery<{ checks: Check[] }>("task.checks", {
		task_id: taskId,
	});
	const checks = data?.checks ?? [];
	if (checks.length === 0 || !checks.every((c) => c.passed)) return null;
	return <span>{t("checksPassed", { count: String(checks.length) })}</span>;
}
