import { Button, Dialog, TextArea } from "@catervas/ui";
import { useRef, useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { commandSaid, daemonSaid, saidAll } from "../../app/refusals.ts";
import { codeOf } from "../../app/words.ts";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";

/** One thing the owner's click does: send a command, or file a request in their words. */
export type Step = { command: object } | { request: string };

/**
 * Does `steps` in order, and calls `onDone` when all are done. A step that was refused stops the
 * rest and is said in words; asking again does the steps that are not done, not the ones that
 * are, so that a command that went through is not sent twice.
 */
export function useSteps(onDone: () => void) {
	const { client } = useConnection();
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const done = useRef(0);
	const send = async (steps: Step[]) => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			for (let at = done.current; at < steps.length; at++) {
				const step = steps[at] as Step;
				if ("command" in step) {
					const reply = await client.command(step.command as never);
					if ("error" in reply) {
						setRefusal(commandSaid(reply.error.detail, {}));
						return;
					}
				} else {
					try {
						await client.call("request.file", { text: step.request });
					} catch (error) {
						// A coded refusal (the length one) is worded by its code.
						setRefusal(
							codeOf(error)
								? saidAll(error)
								: daemonSaid(error, "refuseCommand"),
						);
						return;
					}
				}
				done.current = at + 1;
			}
			onDone();
		} catch {
			// The connection closed: the page shows that it is lost.
		} finally {
			setBusy(false);
		}
	};
	return { busy, refusal, send };
}

/**
 * A request the owner sends the team from a row or a page, in their own words (spec 6.10): the
 * draft is theirs to change before "Send to the team" files it, and `then` is sent after it went
 * through (a renewal's dismissal); a refused request sends nothing after it.
 */
export function AskTeam({
	title,
	draft,
	pm,
	then,
	onClose,
}: {
	title: string;
	draft: string;
	/** The Product Manager's name, who reads every request. */
	pm: string;
	then?: object | undefined;
	onClose: () => void;
}) {
	const [text, setText] = useState(draft);
	const { busy, refusal, send } = useSteps(onClose);
	return (
		<Dialog
			open
			fillsPhone
			title={title}
			onClose={onClose}
			actions={
				<Button
					kind="primary"
					busy={busy}
					disabled={text.trim() === ""}
					onClick={() =>
						send([
							{ request: text.trim() },
							...(then ? [{ command: then }] : []),
						])
					}
				>
					{t("requestSend")}
				</Button>
			}
		>
			<div className={styles.orderForm}>
				<p>{t("askTeamBody")}</p>
				<TextArea
					id="ask-team-text"
					label={t("askTeamText")}
					value={text}
					onChange={setText}
				/>
				<p className={styles.orderHint}>{t("requestHint", { name: pm })}</p>
				{refusal && <p role="alert">{refusal}</p>}
			</div>
		</Dialog>
	);
}
