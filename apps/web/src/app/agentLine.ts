import { t } from "../strings/t.ts";

export type ActivityLine = { state: string; line: string };

/** What an agent is doing, in the words the band and the Team page show: a waiting agent's
 * question is read in its "Waiting on you" row, so here it is only "Waiting on you". */
export function agentLine(one: ActivityLine): string {
	return one.state === "waiting" ? t("statusWaiting") : one.line;
}
