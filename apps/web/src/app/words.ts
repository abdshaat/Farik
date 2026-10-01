import type { Command, Event } from "@farik/protocol-client";
import { uiStrings } from "@farik/ui";
import type { Agent } from "../pages/setup/TeamSetup.tsx";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import { said } from "./refusals.ts";

/** A command's refusal (`already_paused: …`) as its code's plain sentence; the daemon's own detail never shows. */
export const sentence = (detail: string): string =>
	said(/^([a-z_]+): /.exec(detail)?.[1], {}, "refuseCommand");

/** The code a refused call carries in its data (`errors[0].code`), if it carries one. */
export const codeOf = (e: unknown): string | undefined =>
	(e as { data?: { errors?: { code?: string }[] } }).data?.errors?.[0]?.code;

/** A role as a letter says it (the mockups' "Developer"). */
export const roleWord = (role: Agent["role"]) => uiStrings.roleName[role];

/** The active agent in `role`, if the team has one. */
export function active(agents: Agent[], role: Agent["role"]) {
	return agents.find((a) => a.role === role && a.status !== "retired");
}

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

/** Whether `word` is one of the lifecycle's statuses. */
export const isStatus = (word: string): word is TaskStatus => word in WORDS;

/** A move as the task's History tab words it: "Mira moved it to Done.", "Ada sent FRK-2 back." */
export const movedWords = (who: string, to: TaskStatus, task = t("toldIt")) =>
	t(to === "rejected" ? "toldSentBack" : "toldMoved", {
		who,
		task,
		status: statusWord(to),
	});

/** The plain word for a status, from web-ui.md's lifecycle table. */
export function statusWord(
	status: TaskStatus,
	reason?: EscalationReason,
): string {
	if (status === "escalated" && reason === "approval")
		return t("statusWaiting");
	return t(WORDS[status]);
}
