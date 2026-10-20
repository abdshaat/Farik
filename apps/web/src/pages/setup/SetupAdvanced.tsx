import { toCamel, toSnake } from "@catervas/protocol-client";
import { Button, Switch, TextArea, TextField } from "@catervas/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { type Refusal, said, saidAll } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import { Bullets } from "../../components/Bullets.tsx";
import { t } from "../../strings/t.ts";
import { slug } from "./SetupProject.tsx";
import styles from "./setup.module.css";
import {
	type Criterion,
	type Draft,
	PutBack,
	type Team,
	teamOf,
	useDefaults,
	useSetup,
} from "./TeamSetup.tsx";

/** The advanced settings of setup, opened in place on the last screen: team rules and the checks. */
export function AdvancedSettings() {
	const { client } = useConnection();
	const { draft, change } = useSetup();
	const defaults = useDefaults();
	const team = teamOf(draft);
	// Every change is checked as it is made (spec 10); the daemon's refusal shows here.
	const { data: checked } = useQuery<{ errors: Refusal[] }>("team.validate", {
		team,
	});
	const errors = checked?.errors ?? [];
	// Each refusal is said at the group it concerns (SPEC 4.1); the rest at the foot.
	const under = (prefix: string) =>
		errors.filter((e) => e.path.startsWith(prefix));
	const elsewhere = errors.filter((e) => !e.path.startsWith("/rules"));
	const rules = draft.team.rules;
	const [maxCost, setMaxCost] = useState(String(rules.maxTaskBudgetUsd ?? ""));
	const [asText, setAsText] = useState<string>();
	const [textWrong, setTextWrong] = useState<string>();
	const [check, setCheck] = useState("");
	const [adding, setAdding] = useState(false);
	const [checkRefused, setCheckRefused] = useState<string>();

	const withTeam = (next: Partial<Team>): Draft => ({
		...draft,
		team: { ...draft.team, ...next },
	});
	const setRules = (next: Team["rules"]) => change(withTeam({ rules: next }));
	const cost = (value: string) => {
		setMaxCost(value);
		const { maxTaskBudgetUsd: _, ...rest } = rules;
		const dollars = Number(value);
		setRules(
			value.trim() && dollars > 0
				? { ...rest, maxTaskBudgetUsd: dollars }
				: rest,
		);
	};
	const applyText = () => {
		try {
			setRules(toCamel(JSON.parse(asText ?? "")) as Team["rules"]);
			setAsText(undefined);
			setTextWrong(undefined);
		} catch {
			setTextWrong(t("rulesTextWrong"));
		}
	};
	const add = async () => {
		if (!client) return;
		const text = check.trim();
		const criterion: Criterion = {
			name: slug(text),
			text,
			source: "human",
			verification: { method: "review", rubric: [text] },
		};
		const criteria = {
			...draft.criteria,
			criteria: [...draft.criteria.criteria, criterion],
		};
		setAdding(true);
		setCheckRefused(undefined);
		try {
			await client.call("criteria.save", { criteria });
			change({ ...draft, criteria });
			setCheck("");
		} catch (e) {
			setCheckRefused(saidAll(e));
		}
		setAdding(false);
	};

	return (
		<>
			<section className={styles.card} aria-labelledby="rules">
				<span className={styles.split}>
					<h2 id="rules" className={styles.heading}>
						{t("rulesTitle")}
					</h2>
					<Button
						kind="quiet"
						onClick={() =>
							setAsText(
								asText === undefined
									? JSON.stringify(toSnake(rules), null, 2)
									: undefined,
							)
						}
					>
						{t("editAsText")}
					</Button>
				</span>
				<p>{t("rulesLead")}</p>
				{under("/rules").length > 0 && (
					<p className={styles.alert}>
						{under("/rules")
							.map((e) => said(e.code))
							.join(" ")}
					</p>
				)}
				{asText !== undefined ? (
					<>
						<TextArea
							id="rules-text"
							label={t("rulesText")}
							rows={10}
							value={asText}
							onChange={setAsText}
							{...(textWrong && { error: textWrong })}
						/>
						<span>
							<Button onClick={applyText}>{t("rulesTextUse")}</Button>
						</span>
					</>
				) : (
					<>
						<dl className={styles.facts}>
							<div>
								<dt>{t("rulePrivate")}</dt>
								<dd>
									{t("rulePrivateNote")} <small>{t("alwaysOn")}</small>
								</dd>
							</div>
							<div>
								<dt>{t("ruleDeveloper")}</dt>
								<dd>
									{t("ruleDeveloperNote")} <small>{t("alwaysOn")}</small>
								</dd>
							</div>
						</dl>
						<Switch
							id="require-tests"
							label={t("ruleTests")}
							description={t("ruleTestsNote")}
							checked={rules.requireNewTests ?? false}
							onChange={(on) => setRules({ ...rules, requireNewTests: on })}
						/>
						<TextField
							id="max-cost"
							label={t("ruleMaxCost")}
							hint={t("ruleMaxCostHint")}
							value={maxCost}
							onChange={cost}
						/>
					</>
				)}
				<PutBack
					onClick={
						defaults &&
						(() => {
							setRules(defaults.rules);
							setMaxCost(String(defaults.rules.maxTaskBudgetUsd ?? ""));
							setAsText(undefined);
						})
					}
				/>
			</section>

			<section className={styles.card} aria-labelledby="documents">
				<h2 id="documents" className={styles.heading}>
					{t("rulesDocuments")}
				</h2>
				<Switch
					id="documents-local"
					label={t("documentsLocal")}
					info={<Bullets text={t("documentsLocalNote")} />}
					checked={draft.team.policy.documentsLocal === true}
					onChange={(on) =>
						change(
							withTeam({
								policy: { ...draft.team.policy, documentsLocal: on },
							}),
						)
					}
				/>
			</section>

			<section className={styles.card} aria-labelledby="checks">
				<h2 id="checks" className={styles.heading}>
					{t("checksTitle")}
				</h2>
				<p>{t("checksLead")}</p>
				<ul className={styles.rows} aria-label={t("checksTitle")}>
					{draft.criteria.criteria.map((c) => (
						<li key={c.name} className={styles.check}>
							<span>{c.text}</span>
							<small>
								{c.source === "project_scan" && c.verification.command
									? t("checkFound").replace("{command}", c.verification.command)
									: t("checkYours")}
							</small>
						</li>
					))}
					<li className={styles.check}>
						<span>{t("checkReviewed")}</span>
						<small>
							{t("checkReviewedNote")} {t("alwaysOn")}
						</small>
					</li>
				</ul>
				<TextField
					id="new-check"
					label={t("checkNew")}
					hint={t("checkNewHint")}
					value={check}
					onChange={setCheck}
					{...(checkRefused && { error: checkRefused })}
				/>
				<span>
					<Button
						busy={adding}
						disabled={check.trim().length < 10}
						onClick={add}
					>
						{t("checkAdd")}
					</Button>
				</span>
			</section>

			{elsewhere.length > 0 && (
				<p role="alert" className={styles.alert}>
					{elsewhere.map((e) => said(e.code)).join(" ")}
				</p>
			)}
		</>
	);
}
