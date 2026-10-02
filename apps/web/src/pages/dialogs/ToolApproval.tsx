import { Button, Dialog, TextArea } from "@farik/ui";
import { useId, useState } from "react";
import { Link } from "react-router";
import { t } from "../../strings/t.ts";
import type { AllowanceRow, Allowances } from "../allowances.tsx";
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

/**
 * `text` with each control character (a line break and a tab aside) and each character that hides
 * or reorders text written `\u{202e}`, so that what the human reads is what would be sent.
 */
export function visibly(text: string): string {
	return Array.from(text, (character) => {
		const code = character.codePointAt(0) ?? 0;
		const hidden =
			(code < 0x20 && code !== 0x0a && code !== 0x09) ||
			(code >= 0x7f && code <= 0x9f) ||
			code === 0x61c ||
			(code >= 0x200b && code <= 0x200f) ||
			(code >= 0x202a && code <= 0x202e) ||
			(code >= 0x2060 && code <= 0x2064) ||
			(code >= 0x2066 && code <= 0x2069) ||
			code === 0xfeff;
		return hidden ? `\\u{${code.toString(16)}}` : character;
	}).join("");
}

/** "Allow once" or "Don't allow" one call an agent asked to make to a connector (5.7). */
export function ToolApproval({
	ask,
	agent,
	agentId,
	allowance,
	taskId,
	title,
	onClose,
}: {
	ask: ToolAsk;
	agent: string;
	agentId?: string;
	/** What the agent has made of this tool this period, when the tool has an allowance (ADR 0037). */
	allowance?:
		| {
				row: AllowanceRow | undefined;
				period: Allowances["period"];
				service: string;
		  }
		| undefined;
	taskId: string;
	title: string;
	onClose: () => void;
}) {
	const counted = allowance?.row;
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
			fillsPhone
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
				{counted && allowance ? (
					<>
						<p>
							{t("allowApprovalStopped", {
								name: agent,
								what: counted.what,
								service: allowance.service,
							})}
						</p>
						<p>
							{t("allowApprovalCount", {
								name: agent,
								used: counted.used,
								of: counted.of,
								what: counted.what,
								period: t(
									allowance.period.kind === "day"
										? "allowApprovalDay"
										: "allowApprovalSprint",
								),
							})}{" "}
							<Link to={`/team/${agentId}?allowances=${ask.server}`}>
								{t("allowChange")}
							</Link>
						</p>
					</>
				) : (
					<p>{t("toolApprovalStopped", { agent })}</p>
				)}
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
					<pre>{visibly(shown(ask.input))}</pre>
				</section>
				<TextArea
					id="tool-approval-note"
					label={t("toolApprovalNote", { agent })}
					value={note}
					onChange={setNote}
				/>
				<p className={styles.toolHint}>
					{t("toolApprovalOnce", { agent })}
					{counted && ` ${t("allowApprovalOnce")}`}
				</p>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
