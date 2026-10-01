import { describe, expect, it } from "vitest";
import { statusWord } from "./words.ts";

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
