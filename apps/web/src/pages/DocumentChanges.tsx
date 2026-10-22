import { Button, TextArea } from "@catervas/ui";
import { useId, useState } from "react";
import { useCommand } from "../app/command.ts";
import { Link } from "../app/links.tsx";
import { AgentPicture } from "../components/AgentPicture.tsx";
import { t } from "../strings/t.ts";
import styles from "./DocumentChanges.module.css";
import { visibly } from "./dialogs/ToolApproval.tsx";
import { docKey, fileRoute, isHtml } from "./files.ts";
import type { Agent } from "./setup/TeamSetup.tsx";

/**
 * The team's integration policy. Catervas never merges (ADR 0061), so every team reads as
 * `pull_request`; the type keeps its three values for older files and fixtures.
 */
export type Integration = "auto_merge" | "pull_request" | "manual";

/** One card of `folder_doc_proposals.list`: an agent's changes to the product plan after one review. */
export type DocumentChangesCard = {
	sprintId: string;
	agentId: string;
	summary: string;
	documents: {
		proposal: number;
		path: string;
		lines: { change: "same" | "added" | "removed"; text: string }[];
	}[];
};

const NAMES: Record<string, "docNameRoadmap" | "docNameSpec"> = {
	"product/roadmap": "docNameRoadmap",
	"product/spec": "docNameSpec",
};

const ENTITIES: Record<string, string> = {
	"&amp;": "&",
	"&lt;": "<",
	"&gt;": ">",
	"&quot;": '"',
	"&#39;": "'",
};

/**
 * A line as the owner reads it: a heading without its `#`s, a bullet without its mark. An HTML
 * line has its tags removed and five entities decoded; the result is only ever a text node.
 */
function plain(
	text: string,
	path: string,
): { words: string; heading: boolean } {
	if (isHtml(path)) {
		const words = text
			.replace(/<[^>]*>/g, "")
			.replace(/&(?:amp|lt|gt|quot|#39);/g, (e) => ENTITIES[e] ?? e)
			.trim();
		return { words, heading: /^<h[1-6][\s>]/.test(text.trim()) };
	}
	const heading = /^#+\s*/.exec(text);
	if (heading) return { words: text.slice(heading[0].length), heading: true };
	return { words: text.replace(/^[-*+] /, ""), heading: false };
}

/**
 * The product plan's changes the owner approves on Today (spec 5.7): the agent's summary, each
 * document's changed lines, a box for notes, and "Approve" and "Send back". The summary and the
 * lines are the agent's words, shown as text and never as markup.
 */
export function DocumentChanges({
	card,
	name,
	agent,
	again,
}: {
	card: DocumentChangesCard;
	name: string;
	agent?: Agent | undefined;
	again: () => void;
}) {
	const id = useId();
	const [notes, setNotes] = useState("");
	const [needNotes, setNeedNotes] = useState(false);
	const { busy, refusal, send } = useCommand(again);
	const proposals = card.documents.map((one) => one.proposal);
	const note = notes.trim();
	const approve = () =>
		send({
			command: "folder_doc_decide",
			body: {
				proposals,
				decision: "approve",
				...(note === "" ? {} : { note }),
			},
		});
	const sendBack = () => {
		if (note === "") {
			setNeedNotes(true);
			return;
		}
		setNeedNotes(false);
		send({
			command: "folder_doc_decide",
			body: { proposals, decision: "return", note },
		});
	};
	return (
		<section className={styles.card} aria-labelledby={`${id}-title`}>
			<div className={styles.head}>
				<AgentPicture agent={agent} size={48} />
				<div className={styles.headText}>
					<strong id={`${id}-title`}>{t("docChangesTitle")}</strong>
					<p className={styles.muted}>{t("docChangesFrom", { name })}</p>
					<p>{visibly(card.summary)}</p>
				</div>
			</div>
			<p className={styles.muted}>{t("docChangesLegend")}</p>
			{card.documents.map((one) => {
				const named = NAMES[docKey(one.path)];
				return (
					<div key={one.proposal} className={styles.document}>
						<p id={`${id}-doc-${one.proposal}`} className={styles.docName}>
							{named ? t(named) : visibly(one.path)}
						</p>
						<Link
							to={fileRoute(one.path)}
							aria-describedby={`${id}-doc-${one.proposal}`}
							className={styles.open}
						>
							{t("filesOpen")}
						</Link>
						{one.lines.length === 0 ? (
							<p className={styles.muted}>{t("docChangesTeamOnly")}</p>
						) : (
							<ul
								className={styles.lines}
								aria-labelledby={`${id}-doc-${one.proposal}`}
							>
								{one.lines.map((line, at) => {
									const { words, heading } = plain(
										visibly(line.text),
										one.path,
									);
									return (
										<li
											// biome-ignore lint/suspicious/noArrayIndexKey: lines repeat, and never move
											key={at}
											className={`${styles[line.change]} ${heading ? styles.heading : ""}`}
										>
											<span className={styles.mark} aria-hidden="true">
												{line.change === "added"
													? "+"
													: line.change === "removed"
														? "−"
														: ""}
											</span>
											<span>
												{line.change !== "same" && (
													<span className={styles.hidden}>
														{t(
															line.change === "added"
																? "docChangeAdded"
																: "docChangeRemoved",
														)}
													</span>
												)}
												{words}
											</span>
										</li>
									);
								})}
							</ul>
						)}
					</div>
				);
			})}
			<TextArea
				id={`${id}-notes`}
				label={t("docChangesNotes", { name })}
				hint={t("docChangesNotesHint")}
				value={notes}
				onChange={setNotes}
				{...(needNotes ? { error: t("docChangesNeedNotes", { name }) } : {})}
				rows={3}
			/>
			<div className={styles.acts}>
				<Button kind="primary" busy={busy} onClick={approve}>
					{t("docChangesApprove")}
				</Button>
				<Button busy={busy} onClick={sendBack}>
					{t("docChangesSendBack")}
				</Button>
				<span className={styles.policy}>{t("docChangesPullRequest")}</span>
			</div>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
		</section>
	);
}
