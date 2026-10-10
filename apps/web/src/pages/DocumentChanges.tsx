import { Avatar, type AvatarKey, Button, TextArea } from "@catervas/ui";
import { useId, useState } from "react";
import { t } from "../strings/t.ts";
import styles from "./DocumentChanges.module.css";
import { useCommand } from "./dialogs/StartSprint.tsx";
import { visibly } from "./dialogs/ToolApproval.tsx";

/** The team's integration policy, which says what approving does. */
export type Integration = "auto_merge" | "pull_request" | "manual";

/** One card of `folder_doc_proposals.list`: an agent's changes to the product plan after one sprint review. */
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
	"docs/catervas/product/roadmap.md": "docNameRoadmap",
	"docs/catervas/product/spec.md": "docNameSpec",
};

const NOTES: Record<
	Integration,
	"docChangesNothingYet" | "docChangesPullRequest" | "docChangesManual"
> = {
	auto_merge: "docChangesNothingYet",
	pull_request: "docChangesPullRequest",
	manual: "docChangesManual",
};

/** A line as the owner reads it: a heading without its `#`s, a bullet without its mark. */
function plain(text: string): { words: string; heading: boolean } {
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
	integration,
	name,
	avatar,
	again,
}: {
	card: DocumentChangesCard;
	integration: Integration;
	name: string;
	avatar?: AvatarKey | undefined;
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
				{avatar && <Avatar avatarKey={avatar} name={name} size={48} />}
				<div className={styles.headText}>
					<strong id={`${id}-title`}>{t("docChangesTitle")}</strong>
					<p className={styles.muted}>
						{t("docChangesFrom", {
							name,
							n: card.sprintId.replace(/^S/, ""),
						})}
					</p>
					<p>{visibly(card.summary)}</p>
				</div>
			</div>
			<p className={styles.muted}>{t("docChangesLegend")}</p>
			{card.documents.map((one) => {
				const named = NAMES[one.path];
				return (
					<div key={one.proposal} className={styles.document}>
						<p id={`${id}-doc-${one.proposal}`} className={styles.docName}>
							{named ? t(named) : visibly(one.path)}
						</p>
						{one.lines.length === 0 ? (
							<p className={styles.muted}>{t("docChangesTeamOnly")}</p>
						) : (
							<ul
								className={styles.lines}
								aria-labelledby={`${id}-doc-${one.proposal}`}
							>
								{one.lines.map((line, at) => {
									const { words, heading } = plain(visibly(line.text));
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
				<span className={styles.policy}>{t(NOTES[integration])}</span>
			</div>
			{refusal && (
				<p role="alert" className={styles.alert}>
					{refusal}
				</p>
			)}
		</section>
	);
}
