import mark from "@catervas/brand/assets/logo-mark-1254.png";
import { Button } from "@catervas/ui";
import { type ReactNode, useState } from "react";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { RaiseBudget } from "./dialogs/RaiseBudget.tsx";
import { EndPlan, type Plan } from "./MarketingPlan.tsx";
import { longDay, money, today, when } from "./marketing.ts";
import type { Agent } from "./setup/TeamSetup.tsx";
import styles from "./Today.module.css";

/** A budget a plan's ads reached: a campaign's own (with its key and name) or the plan's. */
export type Cap = {
	scope: "campaign" | "plan";
	key?: string;
	name?: string;
	spent: string;
	budget: string;
};

/** What a row about a plan's ads says of the plan (`waiting.list`, step 08g). */
export type AdsAsk = {
	kind: "marketing_budget" | "marketing_ads_running" | "marketing_spend_unread";
	taskId: string;
	agentId: string | null;
	plan: string;
	planTitle: string;
	endsOn: string;
	currency: string;
	googleAds?: string;
	spent?: string;
	readAt?: string;
	reason?: string;
	/** The budget the row's words are about. */
	cap?: Cap;
	/** Every budget the plan's ads reached, oldest first. */
	caps?: Cap[];
	campaigns?: { key: string; name: string; budget: string; spent: string }[];
	/** The task of the owner's open request for a new version with a raised budget. */
	raising?: string;
};

/** The words of Google or of Catervas as a sentence: a full stop after them unless they end in one. */
const sentence = (words: string) => {
	const said = words.trim();
	return /[.!?”"]$/.test(said) ? said : `${said}.`;
};

const ADS = "https://ads.google.com";

/**
 * A row about a plan's ads, Catervas's own: the budget they reached, with "Raise the budget" and "End
 * the plan"; ads that may still be running because Catervas could not pause them; or a spend Catervas
 * cannot read (ADR 0042).
 */
export function AdsRow({
	item,
	agent,
}: {
	item: AdsAsk;
	agent: Agent | undefined;
}) {
	const [dialog, setDialog] = useState<"raise" | "end">();
	const name = agent?.displayName ?? item.agentId ?? "";
	const titleId = `waiting-${item.kind}-${item.plan}`;
	const cash = (amount: string) => money(amount, item.currency);
	const day = longDay(item.endsOn);
	const reason = item.reason ? sentence(item.reason) : "";
	const cap = item.cap ?? item.caps?.at(-1);
	const them = cap?.scope === "campaign" ? "it" : "them";
	const figures = (of: Cap) => ({
		spent: cash(of.spent),
		budget: cash(of.budget),
		currency: item.currency,
	});
	const facts: string[] = [];
	let title = "";
	let line = "";
	if (item.kind === "marketing_budget" && cap) {
		title = t("waitingBudgetTitle", { plan: item.planTitle });
		const subject =
			cap.scope === "campaign"
				? t("waitingBudgetCampaign", {
						name: cap.name ?? cap.key ?? "",
						...figures(cap),
					})
				: t("waitingBudgetPlan", figures(cap));
		const standing = reason
			? t("waitingBudgetStuck", { them, reason })
			: t("waitingBudgetPaused", { them });
		line = [
			subject,
			standing,
			item.raising ? t("waitingBudgetRaising", { name }) : "",
		]
			.filter((part) => part !== "")
			.join(" ");
		facts.push(
			t("waitingBudgetSpent", {
				spent: cash(item.spent ?? "0.00"),
				budget: cash(item.googleAds ?? "0.00"),
				currency: item.currency,
			}),
			t("waitingBudgetEnds", { day }),
		);
	} else if (item.kind === "marketing_ads_running") {
		title = t("waitingRunningTitle", { plan: item.planTitle });
		line = t("waitingRunningLine", { reason, day });
	} else {
		title = t("waitingUnreadTitle", { plan: item.planTitle });
		line = t("waitingUnreadLine", { reason, day });
		if (item.spent && item.readAt)
			facts.push(
				t("waitingUnreadLast", {
					when: when(item.readAt),
					spent: cash(item.spent),
					budget: cash(item.googleAds ?? "0.00"),
					currency: item.currency,
				}),
			);
	}
	return (
		<li className={styles.row}>
			{/* The row is Catervas's own, so it carries Catervas's picture, as the team's agents carry theirs. */}
			<img src={mark} alt={t("brand")} width={32} height={32} />
			<div className={styles.rowText}>
				<strong id={titleId}>{title}</strong>
				<span>{line}</span>
				{facts.length > 0 && (
					<span className={styles.figures}>
						{facts.map((fact) => (
							<span key={fact}>{fact}</span>
						))}
					</span>
				)}
			</div>
			<fieldset className={styles.decide} aria-labelledby={titleId}>
				{/* Ads that may be running unpaused: the owner can stop them where Google keeps them. */}
				{item.reason && (
					<a
						className={styles.action}
						href={ADS}
						target="_blank"
						rel="noopener noreferrer"
					>
						{t("waitingOpenAds")}
					</a>
				)}
				{item.kind === "marketing_budget" && !item.raising && (
					<Button onClick={() => setDialog("raise")}>
						{t("waitingRaise")}
					</Button>
				)}
				{item.kind === "marketing_budget" && (
					<Button onClick={() => setDialog("end")}>{t("marketingEnd")}</Button>
				)}
			</fieldset>
			{dialog && (
				<FromThePlan
					plan={item.plan}
					show={(plan) =>
						dialog === "raise" ? (
							<RaiseBudget
								ask={item}
								name={name}
								total={plan.budget.total}
								onClose={() => setDialog(undefined)}
							/>
						) : (
							<EndPlan
								plan={plan}
								name={name}
								now={today()}
								instant={new Date()}
								onClose={() => setDialog(undefined)}
								onSent={() => setDialog(undefined)}
							/>
						)
					}
				/>
			)}
		</li>
	);
}

/** A dialog that needs the plan whole: asks the daemon for it, and shows the dialog once it has it. */
function FromThePlan({
	plan,
	show,
}: {
	plan: string;
	show: (plan: Plan) => ReactNode;
}) {
	const { data } = useQuery<Plan>("marketing_plan.get", { plan });
	return data ? show(data) : null;
}
