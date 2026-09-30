import { describe, expect, it } from "vitest";
import { LANES, type Lane, laneOf, type TaskRow } from "./lanes.ts";
import type { TaskStatus } from "./words.ts";

const row = (status: TaskStatus, awaitingApproval = false): TaskRow => ({
	taskId: "FRK-1",
	kind: "task",
	title: "A task",
	status,
	risk: "low",
	awaitingApproval,
});

describe("lanes", () => {
	it("puts_every_status_in_its_lane", () => {
		// Spec 5.2's states, placed by web-ui.md's table and the step 09 plan.
		const table: Record<TaskStatus, Lane> = {
			draft: "planning",
			refining: "planning",
			ready: "todo",
			assigned: "todo",
			in_progress: "in_progress",
			rejected: "in_progress",
			blocked: "stuck",
			escalated: "stuck",
			verifying: "review",
			accepted: "done",
			cancelled: "done",
		};
		for (const [status, lane] of Object.entries(table))
			expect(laneOf(row(status as TaskStatus)), status).toBe(lane);
		// An escalated plan awaiting approval waits in Planning; any other escalation is stuck.
		expect(laneOf(row("escalated", true))).toBe("planning");
		expect(laneOf(row("escalated", false))).toBe("stuck");
		expect(LANES).toEqual([
			"planning",
			"todo",
			"in_progress",
			"stuck",
			"review",
			"done",
		]);
	});
});
