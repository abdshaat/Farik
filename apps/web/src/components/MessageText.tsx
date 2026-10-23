import type { ReactNode } from "react";
import { Link } from "../app/links.tsx";
import type { Agent } from "../pages/setup/TeamSetup.tsx";
import { t } from "../strings/t.ts";
import styles from "./MessageText.module.css";

/** A row of `waiting.list`: only its task matters here. */
export type WaitingRow = { taskId: string };

// A mention as the runtime reads it (channel.rs `mentions_in`), or a task id.
const TOKEN = /(^|[^\p{L}\p{N}])@([a-z0-9-]*[a-z0-9])|CTV-[0-9]{1,6}\b/gu;

/**
 * A message's text as React elements, never as HTML (agent text is untrusted): task ids link to
 * their task, with "waiting on you" when one waits on the human; `@human` reads "@you" and an
 * agent's `@<id>` its name, highlighted.
 */
export function renderMessageText(
	text: string,
	team: Agent[],
	waiting: WaitingRow[],
): ReactNode {
	const parts: ReactNode[] = [];
	let at = 0;
	let marked = false;
	for (const match of text.matchAll(TOKEN)) {
		const [whole, before = "", id] = match;
		const start = match.index + before.length;
		let shown: ReactNode;
		if (id === undefined) {
			// Part of a file name ("plans/CTV-1.html"), not a task.
			const end = match.index + whole.length;
			if (
				text[start - 1] === "/" ||
				/^\.[A-Za-z]/.test(text.slice(end, end + 2))
			)
				continue;
			// "waiting on you" once per message: the first link to a waiting task.
			const first: boolean = !marked && waiting.some((w) => w.taskId === whole);
			marked ||= first;
			shown = (
				<span key={start}>
					<Link to={`/tasks/${whole}`}>{whole}</Link>
					{first && (
						<span className={styles.waiting}>
							{" "}
							({t("channelWaitingOnYou")})
						</span>
					)}
				</span>
			);
		} else {
			const who = team.find((a) => a.id === id);
			const name =
				id === "human" ? t("channelAtYou") : who && `@${who.displayName}`;
			// Anyone else's `@` is only text.
			if (!name) continue;
			shown = (
				<mark key={start} className={styles.mention}>
					{name}
				</mark>
			);
		}
		parts.push(text.slice(at, start), shown);
		at = match.index + whole.length;
	}
	parts.push(text.slice(at));
	return parts;
}
