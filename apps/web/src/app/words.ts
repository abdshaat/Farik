import type { Command, Event } from "@farik/protocol-client";
import { uiStrings } from "@farik/ui";
import type { Agent } from "../pages/setup/TeamSetup.tsx";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";

/** "triage_closed: the request is already being planned" -> "The request is already being planned". */
export function sentence(detail: string): string {
	const said = detail.replace(/^[a-z_]+: /, "");
	return said.charAt(0).toUpperCase() + said.slice(1);
}

/** The code a refused call carries in its data (`errors[0].code`), if it carries one. */
export const codeOf = (e: unknown): string | undefined =>
	(e as { data?: { errors?: { code?: string }[] } }).data?.errors?.[0]?.code;

/** A role as a letter says it: the mockups' "Developer", every other role by its name. */
export const roleWord = (role: Agent["role"]) =>
	role === "software_developer" ? t("roleDeveloper") : uiStrings.roleName[role];

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

/** The plain word for a status, from web-ui.md's lifecycle table. */
export function statusWord(
	status: TaskStatus,
	reason?: EscalationReason,
): string {
	if (status === "escalated" && reason === "approval")
		return t("statusWaiting");
	return t(WORDS[status]);
}
