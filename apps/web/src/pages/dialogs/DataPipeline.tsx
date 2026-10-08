import { Button, Dialog, TextArea } from "@farik/ui";
import { useState } from "react";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { useCommand } from "./StartSprint.tsx";
import { visibly } from "./ToolApproval.tsx";

/** One data source the Product Manager passed to the owner, as `waiting.list` gives it. */
export type PipelineAsk = {
	pipeline: number;
	/** What the agent called the source, untrusted. */
	name: string;
	/** What it would give the agent, untrusted. */
	what: string;
	/** The page the agent found it on, untrusted. */
	url: string;
	/** The site of `url` in its ASCII form, which Farik worked out. */
	host: string;
	/** The agent's reason, untrusted. */
	why: string;
	cost: "free" | "paid" | "unknown";
	needsAccount: boolean;
	sendsProjectData: boolean;
	/** The Product Manager's reason, untrusted; absent when Farik passed the request on. */
	reason?: string;
	/** The request the team gets if the owner approves, as it would be filed. */
	requestText: string;
};

/**
 * "Approve" or "Decline" the data source an agent asked for, with a note it reads in its next
 * piece of work (spec 6.10). Approving shows the whole request that is filed in the owner's name,
 * since the agent wrote most of it; it connects nothing and pays for nothing.
 */
export function DataPipeline({
	ask,
	agent,
	approve,
	onClose,
}: {
	ask: PipelineAsk;
	agent: string;
	approve: boolean;
	onClose: () => void;
}) {
	const [note, setNote] = useState("");
	const { busy, refusal, send } = useCommand(onClose);
	const decide = () => {
		const said = note.trim();
		send({
			command: "data_pipeline_decide",
			body: {
				pipeline: ask.pipeline,
				approve,
				...(said ? { note: said } : {}),
			},
		});
	};
	const filedId = `data-pipeline-${ask.pipeline}-filed`;
	return (
		<Dialog
			open
			fillsPhone
			title={t(approve ? "pipelineApproveTitle" : "pipelineDeclineTitle", {
				source: visibly(ask.name),
				name: agent,
			})}
			onClose={onClose}
			actions={
				<Button kind="primary" busy={busy} onClick={decide}>
					{t(approve ? "pipelineApprove" : "pipelineDecline")}
				</Button>
			}
		>
			<div className={styles.orderForm}>
				<p>
					{t(approve ? "pipelineApproveBody" : "pipelineDeclineBody", {
						name: agent,
					})}
				</p>
				{approve && (
					<>
						<span className={styles.pipelineHeading} id={filedId}>
							{t("pipelineFiled")}
						</span>
						{/* Mostly the agent's words: React renders them as text, never as markup. */}
						<fieldset
							aria-labelledby={filedId}
							data-trust="untrusted"
							className={styles.pipelineFrame}
						>
							{visibly(ask.requestText)}
						</fieldset>
					</>
				)}
				<TextArea
					id="data-pipeline-note"
					label={t("pipelineNote", { name: agent })}
					value={note}
					onChange={setNote}
				/>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
