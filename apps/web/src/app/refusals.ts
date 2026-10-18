import { RpcError } from "@catervas/protocol-client";
import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";

/** One refusal as the daemon answers it: where, its words for a log, and its code for a person. */
export type Refusal = { path: string; message: string; code?: string };

/** Each refusal code the daemon gives, and the words a person reads for it (SPEC 10, ADR 0016). */
const WORDS: Record<string, keyof typeof en> = {
	too_many: "refuseTooMany",
	needs_product_manager: "refuseNeedsProductManager",
	repeated_id: "refuseRepeatedId",
	worked: "refuseWorked",
	name: "refuseName",
	keeps_read: "refuseKeepsRead",
	status_from_card: "refuseStatusFromCard",
	integration_branch_name: "refuseBranchName",
	integration_branch_missing: "refuseBranchMissing",
	last_of_role: "refuseLastOfRole",
	too_short: "requestTooShort",
	template_exists: "templateExists",
	template_builtin: "templateBuiltin",
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
	agent_not_active: "refuseAgentNotActive",
	task_too_short: "refuseTaskTooShort",
	triage_refused: "refuseTriage",
	lock_refused: "refuseLock",
	// The owner's decisions on a marketing plan (SPEC 6.5).
	marketing_plan_decided: "refuseMarketingPlanDecided",
	marketing_plan_expired: "refuseMarketingPlanExpired",
	marketing_plan_reason_needed: "refuseMarketingPlanReason",
	unknown_marketing_plan: "refuseMarketingPlanUnknown",
	marketing_plan_not_approved: "refuseMarketingPlanNotApproved",
	marketing_plan_ended: "refuseMarketingPlanEnded",
	// The raise of a marketing budget (SPEC 6.5).
	raise_refused: "refuseRaiseRefused",
	raise_open: "refuseRaiseOpen",
	// The owner's decisions on a post (SPEC 6.5).
	post_not_taken_back: "refusePostNotTakenBack",
	post_being_handed_over: "refusePostBeingHandedOver",
	post_already_out: "refusePostAlreadyOut",
	post_not_going_out: "refusePostNotGoingOut",
	unknown_post: "refusePostUnknown",
	post_decided: "refusePostDecided",
	post_in_the_past: "refusePostInThePast",
	// The owner's decisions on a data pipeline request (SPEC 6.10).
	pipeline_decided: "refusePipelineDecided",
	pipeline_not_escalated: "refusePipelineNotEscalated",
	unknown_pipeline: "refusePipelineUnknown",
	pipeline_note_too_long: "refusePipelineNoteLong",
	// The owner's steps on a purchase order, and on a renewal (SPEC 6.10).
	purchase_order_decided: "refusePurchaseOrderDecided",
	purchase_order_expired: "refusePurchaseOrderExpired",
	purchase_order_not_approved: "refusePurchaseOrderNotApproved",
	purchase_order_placed: "refusePurchaseOrderPlaced",
	purchase_order_not_placed: "refusePurchaseOrderNotPlaced",
	purchase_order_ended: "refusePurchaseOrderEnded",
	unknown_purchase_order: "refusePurchaseOrderUnknown",
	purchase_order_paid_invalid: "refusePurchaseOrderPaid",
	purchase_order_currency_invalid: "refusePurchaseOrderCurrency",
	purchase_order_status_invalid: "refusePurchaseOrderStatus",
	purchase_order_note_too_long: "refusePurchaseOrderNote",
	purchase_order_sending: "refusePurchaseOrderSending",
	renewal_dismissed: "refuseRenewalDismissed",
	unknown_renewal: "refuseRenewalUnknown",
	// The mailbox, the messages to sellers and their replies (SPEC 6.10).
	mailbox_login_failed: "refuseMailboxLoginFailed",
	mailbox_needs_tls: "refuseMailboxNeedsTls",
	mailbox_certificate: "refuseMailboxCertificate",
	mailbox_unreachable: "refuseMailboxUnreachable",
	mailbox_provider_unsupported: "refuseMailboxProvider",
	mailbox_not_connected: "refuseMailboxNotConnected",
	seller_send_limit: "refuseSellerSendLimit",
	seller_message_sent: "refuseSellerMessageSent",
	seller_message_discarded: "refuseSellerMessageDiscarded",
	seller_message_is_an_order: "refuseSellerMessageIsAnOrder",
	seller_message_not_this_order: "refuseSellerMessageNotThisOrder",
	unknown_seller_message: "refuseSellerMessageUnknown",
	seller_message_failed: "refuseSellerMessageFailed",
	seller_message_closed: "refuseSellerMessageClosed",
	seller_message_too_long: "refuseSellerMessageTooLong",
	seller_message_field_invalid: "refuseSellerMessageField",
	mailbox_settings_invalid: "refuseMailboxSettings",
	mailbox_files: "refuseMailboxFiles",
	secret_store_unavailable: "refuseSecretStore",
	seller_reply_dismissed: "refuseSellerReplyDismissed",
	unknown_seller_reply: "refuseSellerReplyUnknown",
	// The owner's decisions on the product plan's changes (SPEC 5.7).
	folder_doc_decided: "refuseFolderDocDecided",
	folder_doc_superseded: "refuseFolderDocSuperseded",
	unknown_folder_doc: "refuseUnknownFolderDoc",
	folder_doc_mixed: "refuseFolderDocMixed",
	folder_doc_reason_needed: "refuseFolderDocReasonNeeded",
	folder_doc_busy: "refuseFolderDocBusy",
	folder_doc_link: "refuseFolderDocLink",
	folder_doc_waiting: "refuseFolderDocWaiting",
	// The owner's save of a document in a role's folder (SPEC 4.5).
	file_busy: "refuseFileBusy",
	marketing_plan: "refuseMarketingPlan",
	change_waiting: "refuseChangeWaiting",
	is_a_link: "refuseIsALink",
	too_large: "refuseTooLarge",
	// The owner's draft of a new project's product plan (SPEC 4.3).
	not_a_new_project: "refuseNotANewProject",
	product_plan_approved: "refusePlanApproved",
	product_plan_open: "refusePlanOpen",
	product_plan_no_writer: "refusePlanNoWriter",
	product_plan_no_reviewer: "refusePlanNoReviewer",
	// A skill's refusals (SPEC 6.7), said at the field they belong to.
	skill_runs_commands: "skillRunsCommands",
	skill_attaches_files: "skillAttachesFiles",
	skill_too_large: "skillTooLarge",
	skill_file_not_text: "skillNotText",
	skill_name_invalid: "skillNameInvalid",
	skill_name_mismatch: "skillNameMismatch",
	skill_description_invalid: "skillDescriptionInvalid",
	skill_name_taken: "skillNameTaken",
	skill_limit_reached: "skillLimitReached",
	// The owner's Start of the documenting work (SPEC 4.1).
	catervafication_started: "refuseCatervaficationStarted",
	catervafication_new_project: "refuseCatervaficationNewProject",
	catervafication_no_product_manager: "refuseCatervaficationNoPm",
	catervafication_no_writers: "refuseCatervaficationNoWriters",
	not_editable: "refuseNotEditable",
	no_words: "refuseNoWords",
	no_owner: "refuseNoOwner",
	no_task_taker: "refuseNoTaskTaker",
	not_html: "refuseNotHtml",
	rewrite_open: "refuseRewriteOpen",
	nothing_to_rewrite: "refuseNothingToRewrite",
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
	["another catervas is already running this project", "setupBusy"],
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
