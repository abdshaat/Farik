import { Button, InfoTip, TextArea } from "@catervas/ui";
import { useState } from "react";
import { Link, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import { roleWord, sentence } from "../app/words.ts";
import { Bullets } from "../components/Bullets.tsx";
import { t } from "../strings/t.ts";
import { Failed } from "./Failed.tsx";
import styles from "./pages.module.css";
import own from "./Questions.module.css";
import type { Agent, Team } from "./setup/TeamSetup.tsx";

type Question = {
	questionId: number;
	agentId: string;
	text: string;
	choices: { label: string; hint?: string }[];
	answer: string | null;
};

/** A task's questions: the answered ones, the one to answer now, and the rest hidden until then. */
export function Questions() {
	const { id = "" } = useParams();
	const { data: team, error: e1 } = useQuery<{ team: Team }>("team.get", {});
	const { data, error: e2 } = useQuery<{ questions: Question[] }>(
		"questions.list",
		{
			task_id: id,
		},
	);
	const failed = e1 ?? e2;
	// A read refused before the page has what it needs: say why (a task that is not there).
	if (!team || !data) return failed ? <Failed error={failed} /> : null;
	const questions = data.questions;
	const current = questions.findIndex((q) => q.answer === null);
	const lead = questions[current] ?? questions[0];
	const agent = team.team.agents.find((a) => a.id === lead?.agentId);
	const name = agent?.displayName ?? lead?.agentId ?? "";
	const back = <Link to="/">{t("backToToday")}</Link>;

	if (!lead)
		return (
			<div className={styles.page}>
				{back}
				<p>{t("questionsNone", { id })}</p>
			</div>
		);

	// One open question on its own reads as AnswerQuestion.
	if (questions.length === 1 && current === 0)
		return (
			<div className={styles.page}>
				{back}
				<div className={styles.section}>
					<div className={styles.titled}>
						<h1 className={styles.title}>
							{t("oneQuestionTitle", { agent: name })}
						</h1>
						<InfoTip id="questions-info">
							<Bullets text={t("aboutQuestionsTips", { agent: name })} />
						</InfoTip>
					</div>
					<p className={styles.muted}>
						{t("oneQuestionLead", { id, agent: name })}
					</p>
				</div>
				<section className={own.question} aria-labelledby="asks">
					<p id="asks" className={styles.muted}>
						{asks(name, agent)}
					</p>
					<Answer question={lead} name={name} own={t("ownWordsOne")} />
				</section>
			</div>
		);

	return (
		<div className={styles.page}>
			{back}
			<div className={styles.section}>
				<h1 className={styles.title}>{t("questionsTitle", { agent: name })}</h1>
				<p className={styles.muted}>
					{t("questionsLead", { id, agent: name })}
				</p>
			</div>
			<ol className={own.list}>
				{questions.map((q, i) => (
					<li key={q.questionId} className={own.question}>
						<span className={own.number}>{i + 1}</span>
						{q.answer !== null ? (
							<div className={own.body}>
								<strong>{q.text}</strong>
								<p>
									{t("youAnswered")} <span>{q.answer}</span>
								</p>
								<span className={own.tag}>{t("answered")}</span>
							</div>
						) : i === current ? (
							<div className={own.body}>
								<Answer
									key={q.questionId}
									question={q}
									name={name}
									own={t("ownWordsMany")}
								/>
							</div>
						) : (
							<p className={styles.muted}>
								{t("laterQuestion", { agent: name, n: String(current + 1) })}
							</p>
						)}
					</li>
				))}
			</ol>
		</div>
	);
}

function asks(name: string, agent: Agent | undefined) {
	return t("asks", {
		agent: name,
		role: agent ? roleWord(agent.role) : "",
	});
}

/** The question to answer now: its choices, the person's own words, or the agent's call. */
function Answer({
	question,
	name,
	own: ownWords,
}: {
	question: Question;
	name: string;
	own: string;
}) {
	const { client } = useConnection();
	const [words, setWords] = useState("");
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const send = async (answer: string) => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const reply = await client.command({
				command: "question_answer",
				body: { questionId: question.questionId, answer },
			});
			if ("error" in reply) setRefusal(sentence(reply.error.detail));
		} catch {
			// The connection closed: the page shows that it is lost.
		} finally {
			setBusy(false);
		}
	};
	const answer = words.trim();
	return (
		<>
			<div className={own.text}>
				{question.text.split(/\n\s*\n/).map((paragraph) => (
					<p key={paragraph}>{paragraph}</p>
				))}
			</div>
			{question.choices.length > 0 && (
				<>
					<p id={`suggests-${question.questionId}`} className={styles.muted}>
						{t("suggests", { agent: name })}
					</p>
					<ul
						aria-labelledby={`suggests-${question.questionId}`}
						className={own.choices}
					>
						{question.choices.map((c, n) => {
							const hintId = `hint-${question.questionId}-${n}`;
							return (
								<li key={c.label}>
									<Button
										busy={busy}
										{...(c.hint ? { "aria-describedby": hintId } : {})}
										onClick={() => send(c.label)}
									>
										{c.label}
									</Button>
									{c.hint && (
										<p id={hintId} className={styles.muted}>
											{c.hint}
										</p>
									)}
								</li>
							);
						})}
					</ul>
				</>
			)}
			<TextArea
				id={`words-${question.questionId}`}
				label={ownWords}
				value={words}
				onChange={setWords}
				rows={3}
			/>
			{refusal && (
				<p role="alert" className={own.alert}>
					{refusal}
				</p>
			)}
			<div className={own.buttons}>
				<Button
					kind="primary"
					busy={busy}
					disabled={answer === ""}
					onClick={() => send(answer)}
				>
					{t("sendAnswer")}
				</Button>
				<Button busy={busy} onClick={() => send(t("decideText"))}>
					{t("letDecide", { agent: name })}
				</Button>
			</div>
		</>
	);
}
