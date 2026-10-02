import { Button, Dialog, TextArea } from "@farik/ui";
import { useId, useState } from "react";
import { Link } from "react-router";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { useCommand } from "./StartSprint.tsx";

/** One connector call waiting to be allowed, as `waiting.list` gives it. */
export type ToolAsk = {
	approval: number;
	server: string;
	tool: string;
	/** The agent's whole input, compact JSON, untrusted. */
	input: string;
};

/**
 * The input laid out for reading, but only when that changes no value: an input that would not
 * read back the same (a number past double precision, a repeated key) is shown as it came.
 */
function shown(input: string): string {
	try {
		const value: unknown = JSON.parse(input);
		if (JSON.stringify(value) === input) return JSON.stringify(value, null, 2);
	} catch {
		// Not JSON: shown as it came.
	}
	return input;
}

/** "Allow once" or "Don't allow" one call an agent asked to make to a connector (5.7). */
export function ToolApproval({
	ask,
	agent,
	taskId,
	title,
	onClose,
}: {
	ask: ToolAsk;
	agent: string;
	taskId: string;
	title: string;
	onClose: () => void;
}) {
	const [note, setNote] = useState("");
	const { busy, refusal, send } = useCommand(onClose);
	const sendHeading = useId();
	const decide = (command: "tool_approve" | "tool_refuse") => {
		const said = note.trim();
		send({
			command,
			body: { approval: ask.approval, ...(said ? { note: said } : {}) },
		});
	};
	return (
		<Dialog
			open
			title={t("waitingToolApproval", { agent, server: ask.server })}
			onClose={onClose}
			actions={
				<>
					<Button busy={busy} onClick={() => decide("tool_refuse")}>
						{t("toolApprovalRefuse")}
					</Button>
					<Button
						kind="primary"
						busy={busy}
						onClick={() => decide("tool_approve")}
					>
						{t("toolApprovalAllow")}
					</Button>
				</>
			}
		>
			<div className={styles.toolApproval}>
				<p>{t("toolApprovalStopped", { agent })}</p>
				<dl>
					<dt>{t("toolApprovalTool")}</dt>
					<dd>
						<code>{ask.tool}</code>
						{t("toolApprovalLabelled", { tag: t("tagExternal") })}
					</dd>
					<dt>{t("toolApprovalService")}</dt>
					<dd>{t("toolApprovalServiceLine", { server: ask.server, agent })}</dd>
					<dt>{t("toolApprovalFor")}</dt>
					<dd>
						<Link to={`/tasks/${taskId}`}>
							{taskId} {title}
						</Link>
					</dd>
				</dl>
				<h3 id={sendHeading}>{t("toolApprovalSend", { agent })}</h3>
				<p className={styles.toolHint}>
					{t("toolApprovalSendHint", { agent })}
				</p>
				{/* Agent-written: React renders it as text, never as markup. */}
				<section
					aria-labelledby={sendHeading}
					data-trust="untrusted"
					className={styles.untrusted}
					// biome-ignore lint/a11y/noNoninteractiveTabindex: a scroll box must take focus to scroll by keyboard
					tabIndex={0}
				>
					<pre>{shown(ask.input)}</pre>
				</section>
				<TextArea
					id="tool-approval-note"
					label={t("toolApprovalNote", { agent })}
					value={note}
					onChange={setNote}
				/>
				<p className={styles.toolHint}>{t("toolApprovalOnce", { agent })}</p>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
