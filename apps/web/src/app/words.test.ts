import { describe, expect, it } from "vitest";
import { en } from "../strings/en.ts";
import { sentence, statusWord } from "./words.ts";

describe("status words", () => {
	it("words_cover_every_status", () => {
		// web-ui.md's lifecycle table, one word per status.
		const table = {
			draft: "Planning",
			refining: "Planning",
			ready: "To do",
			assigned: "To do",
			in_progress: "In progress",
			rejected: "Being reworked",
			verifying: "Review",
			accepted: "Done",
			blocked: "Stuck",
			escalated: "Needs your help",
			cancelled: "Cancelled",
		} as const;
		for (const [status, word] of Object.entries(table))
			expect(statusWord(status as keyof typeof table)).toBe(word);
		expect(statusWord("escalated", "approval")).toBe("Waiting on you");
		expect(statusWord("escalated", "budget")).toBe("Needs your help");
	});
});

describe("refusal sentences", () => {
	it("words_a_command_refusal_by_its_code_and_never_the_daemon_detail", () => {
		const cases: Record<string, string> = {
			"already_paused: the team is already paused": en.refuseAlreadyPaused,
			"not_paused: the team is not paused": en.refuseNotPaused,
			"already_answered: question 7 has its answer": en.refuseAlreadyAnswered,
			"already_accepted: the human accepted CTV-1's result in this verification":
				en.refuseAlreadyAccepted,
			"not_waiting_for_the_human: CTV-1 is accepted, and its result does not wait for the human":
				en.refuseNotWaiting,
			"not_awaiting_approval: CTV-1 is ready and no approval is asked of the human":
				en.refuseNotWaiting,
			"not_escalated: CTV-1 is ready, and only an escalation is resolved":
				en.refuseNotWaiting,
			"review_first: the reviewer has not finished; send back once the review is in":
				en.refuseReviewFirst,
			"criteria_not_run: Catervas has not yet run C1 on the integration branch":
				en.refuseChecksNotRun,
			"criterion_failed: C1 failed on the integration branch: escalate the epic":
				en.refuseChecksFailed,
			"use_human_accept: CTV-1 awaits the human's approval": en.refuseUsePlan,
			"use_escalation_resolve: CTV-1 is escalated": en.refuseUseHelp,
			"extra_tries_only_for_tries: more tries resume the work":
				en.refuseExtraTries,
			"same_status: CTV-1 is already ready": en.refuseSameStatus,
			"agent_retired: Theo has retired, and a past teammate's chat is read-only":
				en.refuseAgentRetired,
			"sprint_open: sprint S2 is open": en.refuseSprintOpen,
			"no_sprint_open: no sprint is open": en.refuseNoSprintOpen,
			"triage_refused: CTV-1 is already ready": en.refuseTriage,
			"lock_refused: CTV-1 is accepted": en.refuseLock,
			// A code the page does not know, a failure with none, a not-found: one plain sentence.
			"not_a_question: event 7 is a task.created, not a question.asked":
				en.refuseCommand,
			"task CTV-9": en.refuseCommand,
			"an answer is blank, and the log is where somebody reads it back":
				en.refuseCommand,
		};
		for (const [detail, words] of Object.entries(cases))
			expect(sentence(detail), detail).toBe(words);
	});
});
