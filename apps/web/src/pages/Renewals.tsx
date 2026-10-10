import { Button } from "@catervas/ui";
import { useState } from "react";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { AskTeam } from "./dialogs/AskTeam.tsx";
import { useCommand } from "./dialogs/StartSprint.tsx";
import { visibly } from "./dialogs/ToolApproval.tsx";
import { aheadDay, calendarDay, type Renewal } from "./orders.ts";
import styles from "./Today.module.css";

/**
 * "Renewals coming up": each renewal in the register whose decision day is two weeks off or
 * nearer, with the owner's "Ask for a review" and "Dismiss", and how many rows of the register
 * Catervas could not read (spec 6.10). Nothing at all when there is nothing to say.
 */
export function RenewalsSection({
	agent,
	pm,
	now,
}: {
	/** The Procurement Specialist, whose register it is. */
	agent: string;
	/** The Product Manager, who reads every request. */
	pm: string;
	now: Date;
}) {
	const { data, again } = useQuery<{ open: Renewal[]; unreadable: number }>(
		"renewals.list",
		{},
	);
	if (!data || (data.open.length === 0 && data.unreadable === 0)) return null;
	return (
		<section className={styles.section} aria-labelledby="renewals-heading">
			<h2 id="renewals-heading">
				{data.open.length === 0
					? t("renewalsTitleNone")
					: t("renewalsTitle", { n: data.open.length })}
			</h2>
			{data.open.length > 0 && (
				<ul className={styles.rows} aria-label={t("renewalsTitleNone")}>
					{data.open.map((renewal) => (
						<RenewalRow
							key={renewal.renewal}
							renewal={renewal}
							agent={agent}
							pm={pm}
							now={now}
							again={again}
						/>
					))}
				</ul>
			)}
			{data.unreadable > 0 && (
				<p className={styles.muted}>
					{data.unreadable === 1
						? t("renewalsUnreadableOne")
						: t("renewalsUnreadable", { n: data.unreadable })}
				</p>
			)}
		</section>
	);
}

/** One renewal: a calendar mark, the vendor and the days, and the two ways to take it off Today. */
function RenewalRow({
	renewal,
	agent,
	pm,
	now,
	again,
}: {
	renewal: Renewal;
	agent: string;
	pm: string;
	now: Date;
	again: () => void;
}) {
	const [asking, setAsking] = useState(false);
	const { busy, refusal, send } = useCommand(again);
	const vendor = visibly(renewal.vendor);
	const titleId = `renewal-${renewal.renewal}`;
	const dismissal = {
		command: "renewal_dismiss",
		body: { renewal: renewal.renewal },
	};
	return (
		<li className={styles.row}>
			<span className={styles.mark} aria-hidden="true">
				<svg
					width="28"
					height="28"
					viewBox="0 0 24 24"
					fill="none"
					stroke="currentColor"
					strokeWidth="2"
					aria-hidden="true"
				>
					<rect x="3" y="5" width="18" height="16" rx="2" />
					<path d="M3 10h18M8 3v4M16 3v4" />
				</svg>
			</span>
			<div className={styles.rowText}>
				<strong id={titleId}>
					{t("renewalLine", {
						vendor,
						day: calendarDay(renewal.renewsOn, now),
					})}
				</strong>
				<span>
					{t("renewalDecide", {
						day: aheadDay(renewal.decideBy, now),
						name: agent,
					})}
				</span>
				{refusal && (
					<p role="alert" className={styles.alert}>
						{refusal}
					</p>
				)}
			</div>
			{/* The group names the renewal, so that each row's buttons are told from the others. */}
			<fieldset className={styles.decide} aria-labelledby={titleId}>
				<Button kind="primary" onClick={() => setAsking(true)}>
					{t("renewalReview")}
				</Button>
				<Button busy={busy} onClick={() => send(dismissal)}>
					{t("renewalDismiss")}
				</Button>
			</fieldset>
			{asking && (
				<AskTeam
					title={t("renewalReviewTitle", { vendor })}
					draft={t("renewalReviewDraft", {
						vendor,
						renewsOn: calendarDay(renewal.renewsOn, now),
						decideBy: calendarDay(renewal.decideBy, now),
					})}
					pm={pm}
					then={dismissal}
					onClose={() => {
						setAsking(false);
						again();
					}}
				/>
			)}
		</li>
	);
}
