import type { TaskStatus } from "./words.ts";

export type Lane =
	| "planning"
	| "todo"
	| "in_progress"
	| "stuck"
	| "review"
	| "done";

/** One row of `tasks.list`, in camelCase. */
export type TaskRow = {
	taskId: string;
	kind: "epic" | "task";
	title: string;
	status: TaskStatus;
	risk: "low" | "medium" | "high";
	awaitingApproval: boolean;
	parent?: string;
	assigneeId?: string;
	sprint?: string;
};

/** The board's lanes, in the mockup's order. */
export const LANES: Lane[] = [
	"planning",
	"todo",
	"in_progress",
	"stuck",
	"review",
	"done",
];

const PLACES: Record<TaskStatus, Lane> = {
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
	// Shown only under "Show cancelled".
	cancelled: "done",
};

/** A task's lane (web-ui.md's table): an escalated plan awaiting approval waits in Planning. */
export function laneOf(task: TaskRow): Lane {
	if (task.status === "escalated" && task.awaitingApproval) return "planning";
	return PLACES[task.status];
}
