import { Button } from "@farik/ui";
import { useState } from "react";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { AskTeam } from "./dialogs/AskTeam.tsx";
import { CloseOrder } from "./dialogs/CloseOrder.tsx";
import { CorrectStatus } from "./dialogs/CorrectStatus.tsx";
import { MarkPlaced } from "./dialogs/MarkPlaced.tsx";
import { MarkReceived } from "./dialogs/MarkReceived.tsx";
import { visibly } from "./dialogs/ToolApproval.tsx";
import { DownloadOrder } from "./OrderRow.tsx";
import {
	afterDays,
	aheadDay,
	amount,
	figure,
	type OrderItem,
	pastDay,
	periodWords,
	statusWords,
	WAITED_DAYS,
} from "./orders.ts";
import styles from "./pages.module.css";

/** The most orders "Recent orders" lists. */
const RECENT = 5;

/** The step the owner is taking on an order, with the order as it was when they began it. */
type Step = {
	kind: "place" | "receive" | "correct" | "close" | "follow";
	order: OrderItem;
};

/** The day a placed order is waited for: the follow-up's, else 30 days after it was placed. */
const expectedOn = (order: OrderItem) =>
	order.status?.expectedOn ??
	(order.placedOn ? afterDays(order.placedOn, WAITED_DAYS) : undefined);

/**
 * "Orders" on the Procurement Specialist's page: the orders approved for the owner to place, the
 * orders placed with where each stands, and the last five that ended. The agent suggests and
 * follows up; the owner places, pays and receives, so every step here is the owner's, and each acts
 * at once, not through "Save changes" (spec 6.10). It lists every order of the project, and each
 * names the agent that set it up, which is the one that follows it up (only that agent's
 * follow-ups count), retired or not, so that a retired agent's paid orders stay in sight.
 */
export function OrdersSection({
	name,
	pm,
	agents,
}: {
	name: string;
	pm: string;
	/** Every agent of the team, retired ones included, to name each order's own. */
	agents: { id: string; displayName: string }[];
}) {
	const { data, again } = useQuery<{ orders: OrderItem[] }>(
		"purchase_orders.list",
		{},
	);
	// The step stays open as it began when the list changes under it, as a refusal must stay said.
	const [step, setStep] = useState<Step>();
	if (!data) return null;
	const now = new Date();
	const orders = data.orders;
	const approved = orders.filter((order) => order.state === "approved");
	const placed = orders.filter((order) => order.state === "placed");
	const recent = orders
		.filter((order) => order.endedAt)
		.sort((a, b) => (b.endedAt ?? "").localeCompare(a.endedAt ?? ""))
		.slice(0, RECENT);
	const begin = (kind: Step["kind"], order: OrderItem) => {
		setStep({ kind, order });
	};
	const who = (order: OrderItem) => nameOf(agents, order);
	return (
		<section className={styles.section} aria-labelledby="orders-heading">
			<h2 id="orders-heading">{t("ordersTitle")}</h2>
			<p className={styles.muted}>{t("ordersLead", { name })}</p>
			{approved.length > 0 && (
				<>
					<h3 id="orders-to-place" className={styles.subheading}>
						{t("ordersToPlace")}
					</h3>
					<ul className={styles.orders} aria-labelledby="orders-to-place">
						{approved.map((order) => (
							<li key={order.order}>
								<OrderHead order={order} agent={who(order)} />
								<p>
									{t("ordersApproved", {
										day: pastDay(order.decidedAt ?? order.draftedAt, now),
									})}
								</p>
								{order.expiresAt && (
									<p className={styles.muted}>
										{t("ordersPlaceBy", {
											day: aheadDay(order.expiresAt, now),
										})}
									</p>
								)}
								<div className={styles.orderActions}>
									<Button kind="primary" onClick={() => begin("place", order)}>
										{t("ordersPlace")}
									</Button>
									<DownloadOrder order={order.order} />
								</div>
							</li>
						))}
					</ul>
				</>
			)}
			{placed.length > 0 && (
				<>
					<h3 id="orders-placed" className={styles.subheading}>
						{t("ordersPlacedTitle")}
					</h3>
					<ul className={styles.orders} aria-labelledby="orders-placed">
						{placed.map((order) => (
							<PlacedOrder
								key={order.order}
								order={order}
								agent={who(order)}
								now={now}
								onStep={begin}
							/>
						))}
					</ul>
				</>
			)}
			{approved.length === 0 && placed.length === 0 && (
				<p className={styles.muted}>{t("ordersNone")}</p>
			)}
			{recent.length > 0 && (
				<>
					<h3 id="orders-recent" className={styles.subheading}>
						{t("ordersRecent")}
					</h3>
					<ul className={styles.ordersRecent} aria-labelledby="orders-recent">
						{recent.map((order) => (
							<li key={order.order}>
								<span className={styles.orderNumber}>PO-{order.order}</span>
								<span className={styles.orderSeller}>
									{visibly(order.seller)}
								</span>
								<span className={styles.muted}>{ended(order, now)}</span>
								<span className={styles.muted}>
									{t("ordersDraftedBy", { name: who(order) })}
								</span>
							</li>
						))}
					</ul>
				</>
			)}
			{step?.kind === "place" && (
				<MarkPlaced
					order={step.order}
					agent={who(step.order)}
					onDone={again}
					onClose={() => setStep(undefined)}
				/>
			)}
			{step?.kind === "receive" && (
				<MarkReceived
					order={step.order}
					agent={who(step.order)}
					onDone={again}
					onClose={() => setStep(undefined)}
				/>
			)}
			{step?.kind === "correct" && (
				<CorrectStatus
					order={step.order}
					agent={who(step.order)}
					onDone={again}
					onClose={() => setStep(undefined)}
				/>
			)}
			{step?.kind === "close" && (
				<CloseOrder
					order={step.order.order}
					agent={who(step.order)}
					onDone={again}
					onClose={() => setStep(undefined)}
				/>
			)}
			{step?.kind === "follow" && (
				<AskTeam
					title={t("followUpTitle", {
						name: who(step.order),
						order: step.order.order,
					})}
					draft={t("followUpDraft", {
						order: step.order.order,
						seller: visibly(step.order.seller),
					})}
					pm={pm}
					onClose={() => setStep(undefined)}
				/>
			)}
		</section>
	);
}

/** The name of the agent that set `order` up: the team's, or its id when the team has no such agent. */
function nameOf(
	agents: { id: string; displayName: string }[],
	order: OrderItem,
): string {
	return (
		agents.find((agent) => agent.id === order.agentId)?.displayName ??
		order.agentId
	);
}

/** An order's number and seller (the agent's words), who set it up, then its total and period. */
function OrderHead({ order, agent }: { order: OrderItem; agent: string }) {
	return (
		<>
			<div className={styles.orderHead}>
				<span className={styles.orderNumber}>PO-{order.order}</span>
				<span className={styles.orderSeller}>{visibly(order.seller)}</span>
			</div>
			<span className={styles.muted}>
				{t("ordersDraftedBy", { name: agent })}
			</span>
			<span>
				<span className={styles.orderAmount}>
					{amount(order.total, order.currency)}
				</span>{" "}
				{periodWords(order.period)}
			</span>
		</>
	);
}

/** A placed order: when and what was paid, what the last follow-up learned, and the owner's steps. */
function PlacedOrder({
	order,
	agent,
	now,
	onStep,
}: {
	order: OrderItem;
	/** The agent that set the order up, whose follow-ups the status is. */
	agent: string;
	now: Date;
	onStep: (kind: Step["kind"], order: OrderItem) => void;
}) {
	const status = order.status;
	const expected = expectedOn(order);
	return (
		<li>
			<OrderHead order={order} agent={agent} />
			<p>
				{t("ordersPlacedOn", { day: pastDay(order.placedOn ?? "", now) })}
				{order.paid &&
					t("ordersPaid", {
						paid: figure(order.paid),
						currency: order.paidCurrency ?? order.currency,
					})}
			</p>
			{status ? (
				<>
					<div className={styles.orderStatus}>
						<span className={styles.orderTag} data-status={status.status}>
							{statusWords(status.status)}
						</span>
						<span>
							{t(status.by === "owner" ? "ordersByYou" : "ordersByAgent", {
								name: agent,
								day: pastDay(status.at, now),
							})}
						</span>
					</div>
					{status.note &&
						(status.by === "owner" ? (
							<p className={styles.orderNote}>{status.note}</p>
						) : (
							// The agent's own words: React renders them as text, never as markup.
							<pre className={styles.orderUntrusted} data-trust="untrusted">
								{visibly(status.note)}
							</pre>
						))}
				</>
			) : (
				<p className={styles.muted}>{t("ordersNoNews")}</p>
			)}
			{order.overdue ? (
				<div className={styles.orderStatus}>
					<span className={styles.orderTag} data-status="overdue">
						{t("ordersOverdue")}
					</span>
					{expected && (
						<span>{t("ordersExpected", { day: aheadDay(expected, now) })}</span>
					)}
				</div>
			) : (
				status?.expectedOn && (
					<p>
						{t("ordersExpected", { day: aheadDay(status.expectedOn, now) })}
					</p>
				)
			)}
			<div className={styles.orderActions}>
				<Button kind="primary" onClick={() => onStep("receive", order)}>
					{t("ordersReceive")}
				</Button>
				<Button onClick={() => onStep("correct", order)}>
					{t("ordersCorrect")}
				</Button>
				<Button onClick={() => onStep("follow", order)}>
					{t("ordersFollowUp")}
				</Button>
				<Button onClick={() => onStep("close", order)}>
					{t("ordersClose")}
				</Button>
			</div>
		</li>
	);
}

/** How an order that ended ended, on the day it did. */
function ended(order: OrderItem, now: Date): string {
	const day = pastDay(order.endedAt ?? "", now);
	if (order.state === "received")
		return order.paid
			? t("ordersReceived", {
					day,
					paid: figure(order.paid),
					currency: order.paidCurrency ?? order.currency,
				})
			: t("ordersReceivedUnpaid", { day });
	if (order.state === "rejected") return t("ordersRejected", { day });
	if (order.state === "closed") return t("ordersClosed", { day });
	return t(order.decidedAt ? "ordersExpiredApproved" : "ordersExpiredDrafted", {
		day,
	});
}
