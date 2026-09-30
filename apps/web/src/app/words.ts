import type { Command, Event } from "@farik/protocol-client";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";

/** Where a task is in its lifecycle (SPEC 5.2). */
export type TaskStatus = Extract<Command["body"], { to: unknown }>["to"];
/** Why a task went to the human (SPEC 5.7): the escalation body's reasons, which name `risk_gate`. */
type ReasonOf<B> = B extends { reason: infer R; detail: string }
	? "risk_gate" extends R
		? R
		: never
	: never;
export type EscalationReason = ReasonOf<Event["body"]>;

const WORDS: Record<TaskStatus, keyof typeof en> = {
	draft: "statusPlanning",
	refining: "statusPlanning",
	ready: "statusToDo",
	assigned: "statusToDo",
	in_progress: "statusInProgress",
	rejected: "statusReworked",
	verifying: "statusReview",
	accepted: "statusDone",
	blocked: "statusStuck",
	escalated: "statusHelp",
	cancelled: "statusCancelled",
};

/** The plain word for a status, from web-ui.md's lifecycle table. */
export function statusWord(
	status: TaskStatus,
	reason?: EscalationReason,
): string {
	if (status === "escalated" && reason === "approval")
		return t("statusWaiting");
	return t(WORDS[status]);
}
