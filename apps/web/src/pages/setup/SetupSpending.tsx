import { Button, Choice, TextField } from "@farik/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";
import { useSetup } from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

type Limit = "none" | "daily";

/** Setup's seventh step: no limit, or a daily one (the field starts at $10). */
export function SetupSpending() {
	const navigate = useNavigate();
	const { draft, change } = useSetup();
	const kept = draft.team.budgets.dailyUsd;
	const [limit, setLimit] = useState<Limit>(kept ? "daily" : "none");
	const [amount, setAmount] = useState(String(kept ?? 10));
	const dollars = Number(amount);
	const wrong = limit === "daily" && !(amount.trim() !== "" && dollars > 0);

	const onward = () => {
		const { dailyUsd: _, ...budgets } = draft.team.budgets;
		change({
			...draft,
			team: {
				...draft.team,
				budgets:
					limit === "daily" ? { ...budgets, dailyUsd: dollars } : budgets,
			},
		});
		navigate("/setup/finish");
	};

	return (
		<Wizard step={6} title={t("spendTitle")} lead={t("spendLead")}>
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
