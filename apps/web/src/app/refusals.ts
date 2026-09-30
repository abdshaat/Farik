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
};

/** A refusal in plain words, `{key}` filled from `fill`: its code's sentence, or a plain one for any other. */
export function said(
	code: string | undefined,
	fill: Record<string, string> = {},
): string {
	let words = t(WORDS[code ?? ""] ?? "refuseOther");
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

/** A command's refusal, whose detail starts with its code (`last_of_role: …`), in plain words. */
export function commandSaid(
	detail: string,
	fill: Record<string, string>,
): string {
	return said(/^([a-z_]+): /.exec(detail)?.[1], fill);
}
