import { describe, expect, it } from "vitest";
import { agentLine } from "./agentLine.ts";

describe("agentLine", () => {
	it("shortens_a_waiting_line_and_keeps_the_others", () => {
		expect(
			agentLine({
				state: "waiting",
				line: "Waiting on you: CTV-24’s last check (C3) needs someone …",
			}),
		).toBe("Waiting on you");
		for (const [state, line] of [
			["working", "Building Hunt for bugs in the code"],
			["resting", "Resting until 15:40 UTC"],
			["paused", "Paused"],
			["idle", "Nothing to do right now"],
		] as const) {
			expect(agentLine({ state, line })).toBe(line);
		}
	});
});
