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
import { active, sentence } from "../app/words.ts";
import { t } from "../strings/t.ts";
import { Failed } from "./Failed.tsx";
import own from "./PlanEditor.module.css";
import type { Contract, Criterion } from "./PlanPage.tsx";
import { riskWord } from "./PlanPage.tsx";
import styles from "./pages.module.css";
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
/** The fields the editor writes; a save sends those the person changed over the latest read. */
const EDITABLE = [
	"intent",
	"summary",
	"requirements",
	"exitCriteria",
	"scope",
	"budget",
	"risk",
	"allowedPaths",
] as const;
type Editable = (typeof EDITABLE)[number];
const same = (a: unknown, b: unknown) =>
	JSON.stringify(a) === JSON.stringify(b);
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
	const { data: team, error: e1 } = useQuery<{ team: Team }>("team.get", {});
	const {
		data: plan,
		again,
		error: e2,
	} = useQuery<{ contract: Contract }>("contract.get", { task_id: id });
	const [said, setSaid] = useState<string>();
	const failed = e1 ?? e2;
	// A read refused before the page has what it needs: say why (a task that is not there).
	if (!team || !plan) return failed ? <Failed error={failed} /> : null;
	const pm = active(team.team.agents, "product_manager");
	const pmName = pm?.displayName ?? uiStrings.roleName.product_manager;
	// One draft per plan: a fresh read updates what the person has not changed (I3).
	return (
		<Editor
			key={id}
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
	// `base` is the plan the draft was last brought up to date with.
	const [base, setBase] = useState(contract);
	const [draft, setDraft] = useState(contract);
	const [budget, setBudget] = useState(String(contract.budget.maxCostUsd));
	const [out, setOut] = useState(contract.scope.outOfScope.join("\n"));
	const [paths, setPaths] = useState(contract.allowedPaths.join("\n"));
	const [clash, setClash] = useState(false);
	const [verdict, setVerdict] = useState<Verdict>();
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();

	// The draft as typed, the budget as read unless its text changed.
	const mine: Contract = {
		...draft,
		scope: { ...draft.scope, outOfScope: lines(out) },
		allowedPaths: lines(paths),
		budget:
			budget === String(base.budget.maxCostUsd)
				? draft.budget
				: { ...draft.budget, maxCostUsd: Number(budget) },
	};
	const ours = EDITABLE.filter((k) => !same(mine[k], base[k]));
	const withOurs = (c: Contract): Contract => ({
		...c,
		...Object.fromEntries(ours.map((k) => [k, mine[k]])),
	});
	// The plan as it would be saved: the latest read, the lock and status among it, since a save
	// never changes them, with the fields this person changed.
	const edited = withOurs(contract);
	const key = JSON.stringify(edited);

	const takeUp = (c: Contract, keep: readonly Editable[]) => {
		setBase(c);
		setDraft(keep.length ? withOurs(c) : c);
		if (!keep.includes("budget")) setBudget(String(c.budget.maxCostUsd));
		if (!keep.includes("scope")) setOut(c.scope.outOfScope.join("\n"));
		if (!keep.includes("allowedPaths")) setPaths(c.allowedPaths.join("\n"));
	};
	// A fresh read: what the person has not changed follows it, what they have is kept, and a
	// field someone else changed too is said. This happens while rendering, not in an effect:
	// an effect would run after the read is on screen, with `ours` as it was before a keystroke
	// typed in between, and so drop that keystroke.
	if (contract !== base) {
		if (
			ours.some(
				(k) => !same(contract[k], base[k]) && !same(contract[k], mine[k]),
			)
		)
			setClash(true);
		takeUp(contract, ours);
	}

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
			setClash(false);
			onSaid(
				reply.backToRefining ? t("savedBack", { pm: pmName }) : t("saved"),
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
					{t("editTitle", { title: contract.title })}
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
							{t(contract.locked ? "lockClosed" : "lockOpen", { pm: pmName })}
						</strong>
						<p className={styles.muted}>
							{t(contract.locked ? "lockClosedHint" : "lockOpenHint", {
								pm: pmName,
							})}
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
							label={t("fieldPart", { n: String(i + 1) })}
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
									label={t("fieldCriterion", { n: c.id })}
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
						{t("verdictCount", {
							n: String(
								verdict.total - new Set(failures.map((f) => f.rule)).size,
							),
							total: String(verdict.total),
						})}
						{failures.length === 1 &&
							` ${t("verdictOne", { plain: first.charAt(0).toLowerCase() + first.slice(1) })}`}
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
			{clash && (
				<div className={own.banner} role="status">
					<p>{t("editClash", { pm: pmName })}</p>
					<Button
						onClick={() => {
							setClash(false);
							takeUp(contract, []);
						}}
					>
						{t("editTakeNew")}
					</Button>
				</div>
			)}
			<div className={styles.actions}>
				<Button kind="primary" busy={busy} onClick={save}>
					{t("editSave")}
				</Button>
			</div>
		</div>
	);
}
