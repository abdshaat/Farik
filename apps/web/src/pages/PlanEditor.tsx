import {
	Button,
	Choice,
	Switch,
	TextArea,
	TextField,
	uiStrings,
} from "@farik/ui";
import { type ReactNode, useEffect, useState } from "react";
import { Link, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import own from "./PlanEditor.module.css";
import type { Contract, Criterion } from "./PlanPage.tsx";
import { riskWord } from "./PlanPage.tsx";
import styles from "./pages.module.css";
import { active, sentence } from "./RequestFiled.tsx";
import { useAdvanced } from "./Settings.tsx";
import type { Team } from "./setup/TeamSetup.tsx";

const CHECK_AFTER_MS = 400;
/** A save changes these in place (`refining`) or sends them back (`escalated`); the work holds the rest. */
const OPEN = ["draft", "refining", "escalated"];
const OVER = ["accepted", "cancelled"];

type Method = "review" | "human" | "command";
/** Every schema error is one failure row, but the schema counts as one check. */
type Verdict = { failures: { rule: string; plain: string }[]; total: number };

const methodOf = (c: Criterion): Method =>
	c.verification.method === "review" || c.verification.method === "human"
		? c.verification.method
		: "command";
const lines = (text: string) =>
	text
		.split("\n")
		.map((l) => l.trim())
		.filter(Boolean);
const next = (ids: string[], prefix: string) =>
	`${prefix}${Math.max(0, ...ids.map((i) => Number(i.slice(1)) || 0)) + 1}`;

/** A field with its contract key in small mono text beside the plain label. */
function Keyed({ name, children }: { name: string; children: ReactNode }) {
	return (
		<div className={own.keyed}>
			<code className={own.key}>{name}</code>
			{children}
		</div>
	);
}

/** The plan editor: plain fields, Farik's check as you type, the lock, and Save. */
export function PlanEditor() {
	const { id = "" } = useParams();
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { data: plan, again } = useQuery<{ contract: Contract }>(
		"contract.get",
		{ task_id: id },
	);
	const [said, setSaid] = useState<string>();
	if (!team || !plan) return null;
	const pm = active(team.team.agents, "product_manager");
	const pmName = pm?.displayName ?? uiStrings.roleName.product_manager;
	// A save refuses stale fields, so each fresh read starts a fresh draft.
	return (
		<Editor
			key={plan.contract.updatedAt}
			id={id}
			contract={plan.contract}
			pmName={pmName}
			said={said}
			onSaid={(words) => {
				setSaid(words);
				again();
			}}
			reread={again}
		/>
	);
}

function Editor({
	id,
	contract,
	pmName,
	said,
	onSaid,
	reread,
}: {
	id: string;
	contract: Contract;
	pmName: string;
	said: string | undefined;
	onSaid: (words: string) => void;
	reread: () => void;
}) {
	const { client } = useConnection();
	const [advanced, setAdvanced] = useAdvanced();
	const [draft, setDraft] = useState(contract);
	const [budget, setBudget] = useState(String(contract.budget.maxCostUsd));
	const [out, setOut] = useState(contract.scope.outOfScope.join("\n"));
	const [paths, setPaths] = useState(contract.allowedPaths.join("\n"));
	const [verdict, setVerdict] = useState<Verdict>();
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();

	// The plan as it would be saved: the lock and the status as last read, since a save never
	// changes them, and the budget as read unless its text changed.
	const edited: Contract = {
		...draft,
		locked: contract.locked,
		status: contract.status,
		scope: { ...draft.scope, outOfScope: lines(out) },
		allowedPaths: lines(paths),
		budget:
			budget === String(contract.budget.maxCostUsd)
				? draft.budget
				: { ...draft.budget, maxCostUsd: Number(budget) },
	};
	const key = JSON.stringify(edited);

	useEffect(() => {
		if (!client) return;
		let live = true;
		const timer = setTimeout(() => {
			client
				.query("contract.check", { task_id: id, contract: JSON.parse(key) })
				.then(
					(answer) => live && setVerdict(answer as Verdict),
					() => {},
				);
		}, CHECK_AFTER_MS);
		return () => {
			live = false;
			clearTimeout(timer);
		};
	}, [client, id, key]);

	const run = async (work: () => Promise<void>) => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			await work();
		} catch (error) {
			setRefusal(sentence((error as Error).message));
		} finally {
			setBusy(false);
		}
	};
	const command = (
		name: "contract_lock" | "contract_unlock" | "task_transition",
		body: object,
	) =>
		run(async () => {
			const reply = await client?.command({ command: name, body } as never);
			if (reply && "error" in reply) setRefusal(sentence(reply.error.detail));
			else reread();
		});
	const save = () =>
		run(async () => {
			const reply = (await client?.call("contract.save", {
				taskId: id,
				contract: edited,
			})) as { backToRefining: boolean };
			onSaid(
				reply.backToRefining
					? t("savedBack").replace("{pm}", pmName)
					: t("saved"),
			);
		});

	const setCriterion = (index: number, change: (c: Criterion) => Criterion) =>
		setDraft((d) => ({
			...d,
			exitCriteria: d.exitCriteria.map((c, i) => (i === index ? change(c) : c)),
		}));
	const withMethod = (c: Criterion, method: Method): Criterion => ({
		...c,
		verification:
			method === "review"
				? { method, rubric: [c.text] }
				: method === "human"
					? { method, question: c.text }
					: { method: "test", command: c.verification.command ?? "" },
	});
	const withText = (c: Criterion, text: string): Criterion => {
		const v = c.verification;
		// A plain check's words are its rubric or its question.
		if (v.method === "review" && (v.rubric?.length ?? 0) <= 1)
			return { ...c, text, verification: { ...v, rubric: [text] } };
		if (v.method === "human")
			return { ...c, text, verification: { ...v, question: text } };
		return { ...c, text };
	};
	const failures = verdict?.failures ?? [];
	const first = failures[0]?.plain ?? "";

	return (
		<div className={styles.page}>
			<Link to={`/tasks/${id}/plan`}>{t("editBack")}</Link>
			<div className={styles.section}>
				<h1 className={styles.title}>
					{t("editTitle").replace("{title}", contract.title)}
				</h1>
				<p className={styles.muted}>{id}</p>
			</div>
			<Switch
				id="plan-advanced"
				label={t("editAdvanced")}
				checked={advanced}
				onChange={setAdvanced}
			/>
			{OVER.includes(contract.status) ? null : OPEN.includes(
					contract.status,
				) ? (
				<div className={own.banner}>
					<div>
						<strong>
							{t(contract.locked ? "lockClosed" : "lockOpen").replace(
								"{pm}",
								pmName,
							)}
						</strong>
						<p className={styles.muted}>
							{t(contract.locked ? "lockClosedHint" : "lockOpenHint").replace(
								"{pm}",
								pmName,
							)}
						</p>
					</div>
					<Button
						busy={busy}
						onClick={() =>
							command(contract.locked ? "contract_unlock" : "contract_lock", {
								taskId: id,
							})
						}
					>
						{t(contract.locked ? "unlockButton" : "lockButton")}
					</Button>
				</div>
			) : (
				<div className={own.banner}>
					<div>
						<strong>{t("holdTitle")}</strong>
						<p className={styles.muted}>{t("holdHint")}</p>
					</div>
					<Button
						busy={busy}
						onClick={() =>
							command("task_transition", {
								taskId: id,
								to: "escalated",
								reason: t("holdReason"),
							})
						}
					>
						{t("holdButton")}
					</Button>
				</div>
			)}
			<Keyed name="intent">
				<TextArea
					id="plan-intent"
					label={t("fieldIntent")}
					hint={t("fieldIntentHint")}
					value={draft.intent}
					onChange={(intent) => setDraft((d) => ({ ...d, intent }))}
					rows={2}
				/>
			</Keyed>
			<Keyed name="summary">
				<TextArea
					id="plan-summary"
					label={t("fieldSummary")}
					value={draft.summary ?? ""}
					onChange={(summary) => setDraft((d) => ({ ...d, summary }))}
					rows={3}
				/>
			</Keyed>
			<Keyed name="requirements">
				<fieldset className={own.group}>
					<legend>{t("fieldParts")}</legend>
					{draft.requirements.map((r, i) => (
						<TextField
							key={r.id}
							id={`plan-${r.id}`}
							label={t("fieldPart").replace("{n}", String(i + 1))}
							value={r.text}
							onChange={(text) =>
								setDraft((d) => ({
									...d,
									requirements: d.requirements.map((one, j) =>
										j === i ? { ...one, text } : one,
									),
								}))
							}
						/>
					))}
					<div>
						<Button
							kind="quiet"
							onClick={() =>
								setDraft((d) => ({
									...d,
									requirements: [
										...d.requirements,
										{
											id: next(
												d.requirements.map((r) => r.id),
												"R",
											),
											text: "",
										},
									],
								}))
							}
						>
							{t("addPart")}
						</Button>
					</div>
				</fieldset>
			</Keyed>
			<Keyed name="exit_criteria">
				<fieldset className={own.group}>
					<legend>{t("fieldCriteria")}</legend>
					<p className={styles.muted}>{t("fieldCriteriaHint")}</p>
					{draft.exitCriteria.map((c, i) => {
						const method = methodOf(c);
						const options: { value: Method; label: string }[] = [
							{ value: "review", label: t("criterionReview") },
							{ value: "human", label: t("criterionHuman") },
						];
						if (advanced)
							options.push({ value: "command", label: t("criterionCommand") });
						return (
							<div key={c.id} className={own.criterion}>
								<TextField
									id={`plan-${c.id}`}
									label={t("fieldCriterion").replace("{n}", c.id)}
									value={c.text}
									onChange={(text) =>
										setCriterion(i, (one) => withText(one, text))
									}
								/>
								{method === "command" && !advanced ? (
									<p className={styles.muted}>{t("criterionCommandLocked")}</p>
								) : (
									<Choice<Method>
										name={`plan-${c.id}-method`}
										legend={t("criterionWho")}
										options={options}
										value={method}
										onChange={(m) =>
											setCriterion(i, (one) => withMethod(one, m))
										}
									/>
								)}
								{method === "command" && advanced && (
									<TextField
										id={`plan-${c.id}-command`}
										label={t("criterionCommandField")}
										value={c.verification.command ?? ""}
										onChange={(value) =>
											setCriterion(i, (one) => ({
												...one,
												verification: { ...one.verification, command: value },
											}))
										}
										required
									/>
								)}
							</div>
						);
					})}
					<div>
						<Button
							kind="quiet"
							onClick={() =>
								setDraft((d) => ({
									...d,
									exitCriteria: [
										...d.exitCriteria,
										{
											id: next(
												d.exitCriteria.map((c) => c.id),
												"C",
											),
											text: "",
											verification: { method: "review", rubric: [""] },
										},
									],
								}))
							}
						>
							{t("addCriterion")}
						</Button>
					</div>
				</fieldset>
			</Keyed>
			<Keyed name="scope.out_of_scope">
				<TextArea
					id="plan-out"
					label={t("fieldOut")}
					hint={t("fieldOutHint")}
					value={out}
					onChange={setOut}
					rows={3}
				/>
			</Keyed>
			<Keyed name="budget.max_cost_usd">
				<TextField
					id="plan-budget"
					label={t("fieldBudget")}
					value={budget}
					onChange={setBudget}
				/>
			</Keyed>
			{advanced && (
				<>
					<Keyed name="risk">
						<Choice<Contract["risk"]>
							name="plan-risk"
							legend={t("fieldRisk")}
							options={(["low", "medium", "high"] as const).map((r) => ({
								value: r,
								label: riskWord(r),
							}))}
							value={draft.risk}
							onChange={(risk) => setDraft((d) => ({ ...d, risk }))}
						/>
					</Keyed>
					<Keyed name="allowed_paths">
						<TextArea
							id="plan-paths"
							label={t("fieldPaths")}
							hint={t("fieldPathsHint")}
							value={paths}
							onChange={setPaths}
							rows={3}
						/>
					</Keyed>
				</>
			)}
			<section
				className={own.verdict}
				aria-labelledby="verdict"
				aria-live="polite"
			>
				<h2 id="verdict">{t("verdictTitle")}</h2>
				{verdict && (
					<p>
						{t("verdictCount")
							.replace(
								"{n}",
								String(
									verdict.total - new Set(failures.map((f) => f.rule)).size,
								),
							)
							.replace("{total}", String(verdict.total))}
						{failures.length === 1 &&
							` ${t("verdictOne").replace("{plain}", first.charAt(0).toLowerCase() + first.slice(1))}`}
					</p>
				)}
				{failures.length > 1 && (
					<>
						<p>{t("verdictMany")}</p>
						<ul>
							{failures.map((f) => (
								<li key={f.plain}>{f.plain}</li>
							))}
						</ul>
					</>
				)}
				<p className={styles.muted}>{t("verdictNote")}</p>
			</section>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
			{said && <p role="status">{said}</p>}
			<div className={styles.actions}>
				<Button kind="primary" busy={busy} onClick={save}>
					{t("editSave")}
				</Button>
			</div>
		</div>
	);
}
