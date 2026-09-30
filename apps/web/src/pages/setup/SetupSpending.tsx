import { Button, Choice, TextField } from "@farik/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";
import { PutBack, type Team, useDefaults, useSetup } from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

type Limit = "none" | "daily";

/**
 * The daily limit's one control, for setup and the Costs page: no limit, or a daily one (the
 * field starts at $10), with "Put back the default". `budgets` gives the budgets with it applied.
 */
export function useDailyLimit(kept: number | undefined) {
	const defaults = useDefaults();
	const [limit, setLimit] = useState<Limit>(kept ? "daily" : "none");
	const [amount, setAmount] = useState(String(kept ?? 10));
	const dollars = Number(amount);
	const wrong = limit === "daily" && !(amount.trim() !== "" && dollars > 0);
	const budgets = (from: Team["budgets"]): Team["budgets"] => {
		const { dailyUsd: _, ...rest } = from;
		return limit === "daily" ? { ...rest, dailyUsd: dollars } : rest;
	};
	const fields = (
		<>
			<div className={styles.card}>
				<Choice<Limit>
					name="limit"
					legend={t("spendChoice")}
					value={limit}
					onChange={setLimit}
					options={[
						{
							value: "none",
							label: t("spendNone"),
							description: t("spendNoneNote"),
						},
						{
							value: "daily",
							label: t("spendDaily"),
							description: t("spendDailyNote"),
						},
					]}
				/>
			</div>
			{limit === "daily" && (
				<div className={styles.form}>
					<TextField
						id="daily"
						label={t("spendAmount")}
						value={amount}
						onChange={setAmount}
						{...(wrong && { error: t("spendAmountWrong") })}
					/>
				</div>
			)}
			<PutBack
				onClick={
					defaults &&
					(() => {
						const usd = defaults.budgets.dailyUsd;
						setLimit(usd ? "daily" : "none");
						setAmount(String(usd ?? 10));
					})
				}
			/>
		</>
	);
	return { wrong, budgets, fields };
}

/** Setup's seventh step: the daily limit. */
export function SetupSpending() {
	const navigate = useNavigate();
	const { draft, change } = useSetup();
	const { wrong, budgets, fields } = useDailyLimit(draft.team.budgets.dailyUsd);

	const onward = () => {
		change({
			...draft,
			team: { ...draft.team, budgets: budgets(draft.team.budgets) },
		});
		navigate("/setup/finish");
	};

	return (
		<Wizard step={6} title={t("spendTitle")} lead={t("spendLead")}>
			{fields}
			<p className={styles.note}>{t("spendFixed")}</p>
			<p className={styles.note}>{t("firstDay")}</p>
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/permissions")}>
					{t("back")}
				</Button>
				<Button kind="primary" disabled={wrong} onClick={onward}>
					{t("continue")}
				</Button>
			</div>
		</Wizard>
	);
}
