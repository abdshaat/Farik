import { toCamel, toSnake } from "@farik/protocol-client";
import { Button, Choice, Switch, TextArea, TextField } from "@farik/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { type Refusal, said, saidAll } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import { slug } from "./SetupProject.tsx";
import styles from "./setup.module.css";
import {
	type Criterion,
	type Draft,
	type Judgment,
	roleName,
	type Team,
	teamOf,
	useSetup,
	useStart,
} from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

/** The questions a plan check may ask, as the team file pins them, and the words shown for each. */
const QUESTIONS = [
	{
		text: "Does the task fit its budget?",
		label: "planBudget",
		note: "planBudgetNote",
	},
	{
		text: "Would its checks notice if the work went wrong the way its intent worries about?",
		label: "planNotice",
		note: "planNoticeNote",
	},
	{
		text: "Is it small enough to finish in one go?",
		label: "planSmall",
		note: "planSmallNote",
	},
] as const;
const JUDGES = ["architect", "scrum_master", "product_manager"] as const;
type Judge = NonNullable<Judgment["judge"]>;

/** "Sol, the Scrum Master": the first active agent of `role`, or who it would be. */
function holder(team: Team, role: (typeof JUDGES)[number]): string | undefined {
	const agent = team.agents.find(
		(a) => a.role === role && a.status === "active",
	);
	return (
		agent &&
		t("planJudgeNamed")
			.replace("{name}", agent.displayName)
			.replace("{role}", roleName(role))
	);
}

/** The Advanced area of setup: team rules, the checks, and how plans are checked. */
export function SetupAdvanced() {
	const { client } = useConnection();
	const navigate = useNavigate();
	const { draft, change } = useSetup();
	const { start, busy, refused } = useStart();
	const team = teamOf(draft);
	// Every change is checked as it is made (spec 10); the daemon's refusal shows here.
	const { data: checked } = useQuery<{ errors: Refusal[] }>("team.validate", {
		team,
	});
	const errors = checked?.errors ?? [];
	// Each refusal is said at the group it concerns (SPEC 4.1); the rest at the foot.
	const under = (prefix: string) =>
		errors.filter((e) => e.path.startsWith(prefix));
	const elsewhere = errors.filter(
		(e) =>
			!["/rules", "/policy/judgment/judge", "/policy/judgment/questions"].some(
				(p) => e.path.startsWith(p),
			),
	);
	const rules = draft.team.rules;
	const judgment: Required<Judgment> = {
		required: "always",
		questions: QUESTIONS.slice(0, 2).map((q) => q.text),
		judge: "auto",
		...draft.team.policy.judgment,
	};
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
	const setJudgment = (next: Partial<Judgment>) =>
		change(
			withTeam({
				policy: { ...draft.team.policy, judgment: { ...judgment, ...next } },
			}),
		);
	const toggle = (text: string) => {
		const on = new Set(judgment.questions);
		if (on.has(text)) on.delete(text);
		else on.add(text);
		const pinned: string[] = QUESTIONS.map((q) => q.text);
		setJudgment({
			questions: [
				...pinned.filter((q) => on.has(q)),
				...judgment.questions.filter((q) => !pinned.includes(q)),
			],
		});
	};
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
		} catch (e) {
			setTextWrong(t("rulesTextWrong").replace("{why}", (e as Error).message));
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
	const auto = JUDGES.map((role) => holder(team, role)).find(Boolean) ?? "";
	const named = (role: "architect" | "scrum_master") =>
		holder(team, role) ?? t("planJudgeNone").replace("{role}", roleName(role));

	return (
		<Wizard step={7} title={t("advanced")} lead={t("advancedLead")}>
			<span>
				<Button onClick={() => navigate("/setup/finish")}>
					{t("advancedHide")}
				</Button>
			</span>

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

			<section className={styles.card} aria-labelledby="plans">
				<h2 id="plans" className={styles.heading}>
					{t("planTitle")}
				</h2>
				<p>{t("planLead")}</p>
				<Switch
					id="plan-required"
					label={t("planRequired")}
					checked={judgment.required === "always"}
					onChange={(on) => setJudgment({ required: on ? "always" : "never" })}
				/>
				<fieldset
					className={styles.questions}
					aria-describedby={
						under("/policy/judgment/questions").length
							? "questions-error"
							: undefined
					}
				>
					<legend>{t("planQuestions")}</legend>
					{QUESTIONS.map((q) => (
						<label key={q.text} className={styles.question}>
							<input
								type="checkbox"
								checked={judgment.questions.includes(q.text)}
								onChange={() => toggle(q.text)}
							/>
							<span>
								{t(q.label)}
								<small>{t(q.note)}</small>
							</span>
						</label>
					))}
					{under("/policy/judgment/questions").length > 0 && (
						<p id="questions-error" className={styles.error}>
							{under("/policy/judgment/questions")
								.map((e) => said(e.code))
								.join(" ")}
						</p>
					)}
				</fieldset>
				<Choice<Judge>
					name="judge"
					legend={t("planJudge")}
					value={judgment.judge}
					onChange={(judge) => setJudgment({ judge })}
					{...(under("/policy/judgment/judge").length > 0 && {
						error: said("judge_not_held", {
							role:
								judgment.judge === "auto"
									? roleName("architect")
									: roleName(judgment.judge),
						}),
					})}
					options={[
						{ value: "auto", label: t("planJudgeAuto").replace("{who}", auto) },
						{ value: "architect", label: named("architect") },
						{ value: "scrum_master", label: named("scrum_master") },
					]}
				/>
			</section>

			{(elsewhere.length > 0 || refused) && (
				<p role="alert" className={styles.alert}>
					{[...elsewhere.map((e) => said(e.code)), refused]
						.filter(Boolean)
						.join(" ")}
				</p>
			)}
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/finish")}>{t("back")}</Button>
				<Button
					kind="primary"
					busy={busy}
					disabled={errors.length > 0}
					onClick={start}
				>
					{t("startTeam")}
				</Button>
			</div>
		</Wizard>
	);
}
