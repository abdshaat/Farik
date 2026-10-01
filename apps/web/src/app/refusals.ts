import { RpcError } from "@farik/protocol-client";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";

/** One refusal as the daemon answers it: where, its words for a log, and its code for a person. */
export type Refusal = { path: string; message: string; code?: string };

/** Each refusal code the daemon gives, and the words a person reads for it (SPEC 10, ADR 0016). */
const WORDS: Record<string, keyof typeof en> = {
	too_many: "refuseTooMany",
	needs_product_manager: "refuseNeedsProductManager",
	needs_developer: "refuseNeedsDeveloper",
	repeated_id: "refuseRepeatedId",
	worked: "refuseWorked",
	name: "refuseName",
	keeps_read: "refuseKeepsRead",
	status_from_card: "refuseStatusFromCard",
	judge_not_held: "refuseJudge",
	no_questions: "refuseNoQuestions",
	question_length: "refuseQuestionLength",
	last_of_role: "refuseLastOfRole",
	last_judge: "refuseLastJudge",
	too_short: "requestTooShort",
	template_exists: "templateExists",
	template_name: "templateName",
	template_unreadable: "templateUnreadable",
	no_state_folder: "templateNoFolder",
	template_changed: "templateChanged",
	// The human's commands (SPEC 4.2), refused by the orchestrator with `code: detail`.
	already_paused: "refuseAlreadyPaused",
	not_paused: "refuseNotPaused",
	already_answered: "refuseAlreadyAnswered",
	already_accepted: "refuseAlreadyAccepted",
	not_waiting_for_the_human: "refuseNotWaiting",
	not_awaiting_approval: "refuseNotWaiting",
	not_escalated: "refuseNotWaiting",
	review_first: "refuseReviewFirst",
	criteria_not_run: "refuseChecksNotRun",
	criterion_failed: "refuseChecksFailed",
	use_human_accept: "refuseUsePlan",
	use_escalation_resolve: "refuseUseHelp",
	extra_tries_only_for_tries: "refuseExtraTries",
	same_status: "refuseSameStatus",
	agent_retired: "refuseAgentRetired",
	sprint_open: "refuseSprintOpen",
	no_sprint_open: "refuseNoSprintOpen",
	triage_refused: "refuseTriage",
	lock_refused: "refuseLock",
};

/** A refusal in plain words, `{key}` filled from `fill`: its code's sentence, or `other` for any other. */
export function said(
	code: string | undefined,
	fill: Record<string, string> = {},
	other: keyof typeof en = "refuseOther",
): string {
	let words = t(WORDS[code ?? ""] ?? other);
	for (const [key, value] of Object.entries(fill))
		words = words.replaceAll(`{${key}}`, value);
	return words;
}

/** The refusals a failed call carries, or the failure alone, with no code, when it carries none. */
export function refusalsOf(e: unknown): Refusal[] {
	const errors =
		e instanceof RpcError
			? (e.data as { errors?: Refusal[] } | undefined)?.errors
			: undefined;
	return errors?.length
		? errors
		: [{ path: "", message: e instanceof Error ? e.message : String(e) }];
}

/** Every refusal a failed call carries, each in plain words, once. */
export function saidAll(e: unknown): string {
	return [...new Set(refusalsOf(e).map((r) => said(r.code)))].join(" ");
}

/**
 * The daemon's refusals that carry no code, by how its sentence starts (crates/cli/src/setup.rs,
 * crates/runtime/src/daemon/setup.rs, computer.rs, credential.rs, daemon/gates.rs), and the words
 * a person reads for each.
 */
const SENTENCES: [string, keyof typeof en][] = [
	["that folder is not there", "setupNoFolder"],
	["that folder is outside your home folder", "setupOutsideHome"],
	["that folder cannot be read", "setupUnreadable"],
	["your home folder cannot be read", "setupUnreadable"],
	["that folder is not a git project", "setupNotGit"],
	["that folder is inside a git project", "setupInsideGit"],
	["another farik is already running this project", "setupBusy"],
	["connect your AI account first", "setupNoAccount"],
	["a project's name is", "setupName"],
	["say a little more about the project", "setupDescribeMore"],
	["that description is too long", "setupDescribeLess"],
	["a folder with that name is already there", "setupNameTaken"],
	["git is not installed", "setupNoGit"],
	["that is not a Claude subscription token", "setupNotSubscription"],
	["that is not an Anthropic API key", "setupNotApiKey"],
	["this computer has no keychain", "setupNoKeep"],
	["your AI account's key comes from", "setupKeyFromEnvironment"],
	["Docker is not installed", "setupNoDocker"],
	["the sandbox image could not be built", "setupBuildFailed"],
	["the browser could not be fetched", "setupPullFailed"],
	["the team is working to this plan", "planHoldFirst"],
	["the request was already sent, as", "requestSentAs"],
];

/** A failed call with no code, in plain words: the daemon's sentence as `SENTENCES` words it, else `other`; never the daemon's own. */
export function daemonSaid(e: unknown, other: keyof typeof en): string {
	const message = e instanceof Error ? e.message : String(e);
	const known = SENTENCES.find(([start]) => message.startsWith(start));
	return t(known?.[1] ?? other, {
		id: /[A-Z][A-Z0-9]*-\d+/.exec(message)?.[0] ?? "",
	});
}

/** A command's refusal, whose detail starts with its code (`last_of_role: …`), in plain words. */
export function commandSaid(
	detail: string,
	fill: Record<string, string>,
): string {
	return said(/^([a-z_]+): /.exec(detail)?.[1], fill);
}
