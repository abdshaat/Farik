import { toSnake } from "@farik/protocol-client";
import { Button, Dialog, TextArea, uiStrings } from "@farik/ui";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import type { TaskStatus } from "../app/words.ts";
import { t } from "../strings/t.ts";
import own from "./PlanPage.module.css";
import styles from "./pages.module.css";
import { active, sentence } from "./RequestFiled.tsx";
import type { Agent, Team } from "./setup/TeamSetup.tsx";

type Verification = {
	method: string;
	command?: string;
	rubric?: string[];
	question?: string;
};
export type Criterion = {
	id: string;
	text: string;
	satisfies?: string[];
	verification: Verification;
};
/** A plan as `contract.get` answers it, in camelCase; only the fields the pages read are named. */
export type Contract = {
	id: string;
	title: string;
	kind?: "task" | "epic";
	intent: string;
	summary?: string;
	scope: { inScope: string[]; outOfScope: string[] };
	requirements: { id: string; text: string }[];
	exitCriteria: Criterion[];
	assigneeRole: string;
	reviewerRole?: Agent["role"];
	risk: "low" | "medium" | "high";
	budget: { maxCostUsd: number };
	allowedPaths: string[];
	status: TaskStatus;
	locked?: boolean | undefined;
	createdAt?: string;
	updatedAt?: string;
};
type Check = { criterionId: string; text: string; passed: boolean };

export const riskWord = (risk: Contract["risk"]) =>
	t(risk === "low" ? "riskLow" : risk === "high" ? "riskHigh" : "riskMedium");

/** "Thursday 24 September", in UTC, or "Not yet" without a time. */
export const day = (iso?: string) =>
	iso
		? new Date(iso).toLocaleDateString("en-GB", {
				weekday: "long",
				day: "numeric",
				month: "long",
				timeZone: "UTC",
			})
		: t("notYet");

/** A JSON value as YAML, for reading only: strings are quoted where YAML would misread them. */
export function yaml(value: unknown, pad = ""): string {
	const nested = (v: unknown) =>
		v !== null && typeof v === "object" && Object.keys(v as object).length > 0;
	const scalar = (v: unknown) => {
		if (Array.isArray(v)) return "[]";
		if (v !== null && typeof v === "object") return "{}";
		if (typeof v !== "string") return String(v);
		return v === "" || /^[-?:,[\]{}#&*!|>'"%@`\s]|: | #|\s$/.test(v)
			? JSON.stringify(v)
			: v;
	};
	if (Array.isArray(value))
		return value
			.map(
				(item) =>
					`${pad}- ${nested(item) ? yaml(item, `${pad}  `).trimStart() : scalar(item)}`,
			)
			.join("\n");
	return Object.entries(value as object)
		.map(([key, v]) =>
			nested(v)
				? `${pad}${key}:\n${yaml(v, `${pad}  `)}`
				: `${pad}${key}: ${scalar(v)}`,
		)
		.join("\n");
}

/** A plan awaiting approval, read as a letter, with Farik's checks and the two answers. */
export function PlanPage() {
	const { id = "" } = useParams();
	const { client } = useConnection();
	const navigate = useNavigate();
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { data: plan } = useQuery<{ contract: Contract }>("contract.get", {
		task_id: id,
	});
	const { data: checked } = useQuery<{ checks: Check[] }>("task.checks", {
		task_id: id,
	});
	const { data: asked } = useQuery<{ questions: { answer: string | null }[] }>(
		"questions.list",
		{ task_id: id },
	);
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const [asking, setAsking] = useState(false);
	const [note, setNote] = useState("");
	if (!team || !plan || !checked || !asked) return null;

	const contract = plan.contract;
	const agents = team.team.agents;
	const pm = active(agents, "product_manager");
	const pmName = pm?.displayName ?? uiStrings.roleName.product_manager;
	const breaker = active(agents, "scrum_master") ?? pm;
	const epic = contract.kind === "epic";
	const loose = contract.exitCriteria.filter((c) => !c.satisfies?.length);
	const doneWhen = (c: Criterion) =>
		t("planDoneWhen").replace(
			"{text}",
			c.text.charAt(0).toLowerCase() + c.text.slice(1),
		);

	const send = async (
		body:
			| { command: "human_accept"; body: object }
			| { command: "human_send_back"; body: object },
	) => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const reply = await client.command(body as never);
			if ("error" in reply) setRefusal(sentence(reply.error.detail));
			else navigate("/");
		} catch {
			// The connection closed: the page shows that it is lost.
		} finally {
			setBusy(false);
			setAsking(false);
		}
	};

	return (
		<div className={styles.page}>
			<Link to="/">{t("backToToday")}</Link>
			<div className={styles.section}>
				<h1 className={styles.title}>
					{t("approveTitle").replace("{title}", contract.title)}
				</h1>
				<p className={styles.muted}>
					{t(epic ? "approveLeadEpic" : "approveLead").replace("{id}", id)}
				</p>
			</div>
			<section className={own.letter} aria-labelledby="signed">
				<p id="signed" className={styles.muted}>
					{t("planSigned").replace("{name}", pmName)}
				</p>
				<p>{contract.summary ?? contract.intent}</p>
			</section>
			<section className={styles.section}>
				<h2 id="parts">
					{t("planParts").replace("{n}", String(contract.requirements.length))}
				</h2>
				<p className={styles.muted}>{t("planPartsHint")}</p>
				<ol aria-labelledby="parts" className={own.parts}>
					{contract.requirements.map((r) => (
						<li key={r.id}>
							<p>{r.text}</p>
							{contract.exitCriteria
								.filter((c) => c.satisfies?.includes(r.id))
								.map((c) => (
									<p key={c.id} className={own.done}>
										{doneWhen(c)}
									</p>
								))}
						</li>
					))}
				</ol>
				{loose.length > 0 && (
					<>
						<h3 id="whole">{t("planWholeDoneWhen")}</h3>
						<ul aria-labelledby="whole">
							{loose.map((c) => (
								<li key={c.id}>{c.text}</li>
							))}
						</ul>
					</>
				)}
			</section>
			<section className={styles.section}>
				<h2 id="out">{t("planOut")}</h2>
				<ul aria-labelledby="out">
					{contract.scope.outOfScope.map((line) => (
						<li key={line}>{line}</li>
					))}
				</ul>
			</section>
			<section className={styles.section} aria-labelledby="checked">
				<h2 id="checked">{t("planChecked")}</h2>
				{checked.checks.length === 0 ? (
					<p>{t("planCheckedAll")}</p>
				) : (
					<ul className={own.checks}>
						{checked.checks.map((c) => (
							<li key={c.criterionId}>
								<span>{c.text}</span>
								<span className={styles.muted}>
									{t(c.passed ? "checkPassed" : "checkNotYet")}
								</span>
							</li>
						))}
					</ul>
				)}
			</section>
			<details className={own.written}>
				<summary>{t("planWritten")}</summary>
				<pre>{yaml(toSnake(contract))}</pre>
			</details>
			<section className={styles.section} aria-labelledby="about">
				<h2 id="about">{t("planAbout")}</h2>
				<dl className={own.facts}>
					<dt>{t("youAsked")}</dt>
					<dd>{day(contract.createdAt)}</dd>
					<dt>{t("approveQuestions")}</dt>
					<dd>
						{t("planAnswered").replace(
							"{n}",
							String(asked.questions.filter((q) => q.answer !== null).length),
						)}
					</dd>
					<dt>{t("planRisk")}</dt>
					<dd>{riskWord(contract.risk)}</dd>
					<dt>{t("planEstimate")}</dt>
					<dd>${contract.budget.maxCostUsd.toFixed(2)}</dd>
					<dt>{t("planBuilders")}</dt>
					<dd>
						{contract.assigneeRole === "human"
							? t("you")
							: uiStrings.roleName[
									contract.assigneeRole as keyof typeof uiStrings.roleName
								]}
					</dd>
				</dl>
			</section>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
			<div className={styles.actions}>
				<Link to={`/tasks/${id}/plan/edit`}>{t("planEdit")}</Link>
				{contract.status === "escalated" && (
					<>
						<Button
							kind="primary"
							busy={busy}
							onClick={() =>
								send({
									command: "human_accept",
									body: { taskId: id, subject: "contract" },
								})
							}
						>
							{t("planApprove")}
						</Button>
						<Button busy={busy} onClick={() => setAsking(true)}>
							{t("planAskChanges")}
						</Button>
					</>
				)}
			</div>
			{epic && contract.status === "escalated" && (
				<p className={styles.muted}>
					{t("planApproveNote").replace(
						"{breaker}",
						breaker?.displayName ?? pmName,
					)}
				</p>
			)}
			<Dialog
				open={asking}
				title={t("planAskTitle").replace("{pm}", pmName)}
				onClose={() => setAsking(false)}
				actions={
					<Button
						kind="primary"
						busy={busy}
						disabled={note.trim() === ""}
						onClick={() =>
							send({
								command: "human_send_back",
								body: {
									taskId: id,
									subject: "contract",
									message: note.trim(),
									failedCriteria: [],
								},
							})
						}
					>
						{t("planAskSend")}
					</Button>
				}
			>
				<TextArea
					id="change-note"
					label={t("planAskNote")}
					value={note}
					onChange={setNote}
					required
				/>
			</Dialog>
		</div>
	);
}
