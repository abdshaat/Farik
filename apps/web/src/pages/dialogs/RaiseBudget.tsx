import { Button, Dialog, TextField } from "@farik/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { saidAll } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import type { AdsAsk, Cap } from "../AdsRow.tsx";
import { listed, longDay, money } from "../marketing.ts";
import styles from "../pages.module.css";

/** Hundredths of a currency unit in `typed`, or nothing when it is no amount like `500` or `500.5`. */
function hundredths(typed: string): number | undefined {
	const match = /^(\d+)(?:\.(\d{1,2}))?$/.exec(typed.trim());
	if (!match) return undefined;
	return Number(match[1]) * 100 + Number((match[2] ?? "").padEnd(2, "0"));
}

/** Hundredths as the daemon takes an amount: "500.00". */
const wire = (cents: number) =>
	`${Math.floor(cents / 100)}.${String(cents % 100).padStart(2, "0")}`;

/** The amount that `amount` (a decimal string) is, in hundredths. */
const cents = (amount: string) => hundredths(amount) ?? 0;

/**
 * "Raise the budget" of a plan whose ads reached a budget: the new Google Ads budget and the new
 * budget of each campaign that reached its own. What the daemon would refuse is refused here first,
 * once the owner asks, with no call; the new version itself is the agent's to write (ADR 0042).
 */
export function RaiseBudget({
	ask,
	name,
	total,
	onClose,
}: {
	ask: AdsAsk;
	/** The agent that proposed the plan, who writes the new version. */
	name: string;
	/** The plan's total budget now, which rises by what the Google Ads budget does. */
	total: string;
	onClose: () => void;
}) {
	const { client } = useConnection();
	const cash = (amount: string) => money(amount, ask.currency);
	const currency = ask.currency;
	// The campaigns that reached their own budget (the plan's own cap names no key), once each, in
	// the order they did.
	const capped = (ask.caps ?? []).filter(
		(cap, i, all): cap is Cap & { key: string } =>
			cap.key !== undefined &&
			all.findIndex((one) => one.key === cap.key) === i,
	);
	const nameOf = (cap: Cap) => cap.name ?? cap.key ?? "";
	const [google, setGoogle] = useState(ask.googleAds ?? "");
	const [typed, setTyped] = useState<Record<string, string>>(
		Object.fromEntries(capped.map((cap) => [cap.key, cap.budget])),
	);
	const [tried, setTried] = useState(false);
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();

	const spentIn = (key: string, cap: Cap) =>
		ask.campaigns?.find((one) => one.key === key)?.spent ?? cap.spent;
	const googleCents = hundredths(google);
	const chosen = capped.map((cap) => hundredths(typed[cap.key] ?? ""));
	// The campaigns' budgets once raised: the ones raised take what was typed, the others keep theirs.
	const sum = (ask.campaigns ?? []).reduce((all, one) => {
		const at = capped.findIndex((cap) => cap.key === one.key);
		return all + (at >= 0 ? (chosen[at] ?? 0) : cents(one.budget));
	}, 0);

	/** What the daemon would refuse for the Google Ads budget, in the order it checks. */
	const googleFault = () => {
		if (googleCents === undefined) return t("raiseNotAnAmount");
		if (googleCents <= cents(ask.spent ?? "0"))
			return t("raiseMoreThanSpent", {
				spent: cash(ask.spent ?? "0.00"),
				currency,
			});
		if (googleCents < cents(ask.googleAds ?? "0"))
			return t("raiseAtLeastNow", {
				budget: cash(ask.googleAds ?? "0.00"),
				currency,
			});
		if (chosen.every((one) => one !== undefined) && sum > googleCents)
			return t("raiseSumOver", { sum: cash(wire(sum)), currency });
		return undefined;
	};
	const campaignFault = (cap: Cap & { key: string }, at: number) => {
		const value = chosen[at];
		if (value === undefined) return t("raiseNotAnAmount");
		if (value <= cents(spentIn(cap.key, cap)))
			return t("raiseMoreThanSpent", {
				spent: cash(spentIn(cap.key, cap)),
				currency,
			});
		return undefined;
	};
	const faults = {
		google: googleFault(),
		campaigns: capped.map(campaignFault),
	};
	const refused = faults.google !== undefined || faults.campaigns.some(Boolean);

	const names =
		capped.length > 0 ? listed(capped.map(nameOf)) : t("raiseItsAds");
	const lead =
		capped.length === 0
			? t("raiseLeadPlan", { agent: name })
			: t(capped.length === 1 ? "raiseLeadCampaign" : "raiseLeadCampaigns", {
					names,
					agent: name,
				});
	const newTotal =
		googleCents !== undefined && googleCents >= cents(ask.googleAds ?? "0")
			? cents(total) + googleCents - cents(ask.googleAds ?? "0")
			: cents(total);
	const send = async () => {
		setTried(true);
		if (refused || googleCents === undefined || !client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			await client.call("marketing_budget.raise", {
				plan: ask.plan,
				googleAds: wire(googleCents),
				campaigns: capped.map((cap, at) => ({
					key: cap.key,
					budget: wire(chosen[at] ?? 0),
				})),
			});
			onClose();
		} catch (error) {
			setRefusal(saidAll(error));
			setBusy(false);
		}
	};
	return (
		<Dialog
			open
			fillsPhone
			title={t("raiseTitle", { plan: ask.planTitle })}
			onClose={onClose}
			actions={
				<Button kind="primary" busy={busy} onClick={send}>
					{t("raiseSend", { agent: name })}
				</Button>
			}
		>
			<p>{lead}</p>
			<TextField
				id="raise-google-ads"
				label={t("raiseGoogleAds")}
				value={google}
				onChange={setGoogle}
				hint={t("raiseGoogleAdsHint", {
					budget: cash(ask.googleAds ?? "0.00"),
					currency,
					spent: cash(ask.spent ?? "0.00"),
				})}
				{...(tried && faults.google ? { error: faults.google } : {})}
			/>
			{capped.map((cap, at) => (
				<TextField
					key={cap.key}
					id={`raise-${cap.key}`}
					label={nameOf(cap)}
					value={typed[cap.key] ?? ""}
					onChange={(value) => setTyped({ ...typed, [cap.key]: value })}
					hint={
						cents(spentIn(cap.key, cap)) >= cents(cap.budget)
							? t("raiseCampaignHint", {
									budget: cash(cap.budget),
									currency,
								})
							: t("raiseCampaignHintSome", {
									budget: cash(cap.budget),
									currency,
									spent: cash(spentIn(cap.key, cap)),
								})
					}
					{...(tried && faults.campaigns[at]
						? { error: faults.campaigns[at] as string }
						: {})}
				/>
			))}
			<p className={styles.muted}>
				{t("raiseTotal", {
					from: cash(total),
					to: cash(wire(newTotal)),
					currency,
				})}
			</p>
			<h3 className={styles.subheading}>{t("raiseNext")}</h3>
			<ul>
				<li>
					{t("raiseNextWrites", { agent: name, day: longDay(ask.endsOn) })}
				</li>
				<li>
					{t(capped.length === 1 ? "raiseNextWaits" : "raiseNextWaitsMany", {
						names,
					})}
				</li>
				<li>
					{t("raiseNextApproved", {
						agent: name,
						names: capped.length > 0 ? names : t("raiseItsAdsAgain"),
					})}
				</li>
			</ul>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
		</Dialog>
	);
}
