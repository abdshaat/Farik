/** What the owner does about a row of "Waiting on you": set something up, answer, or merge. */
export type WaitingGroup = "set_up" | "answer" | "merge";

/** The groups in the order Today shows them: what stops work first. */
export const GROUP_ORDER: readonly WaitingGroup[] = [
	"set_up",
	"answer",
	"merge",
];

/** How many rows of the merge group show before "Show all". */
export const MERGE_SHOWN = 3;

/** Which group a `waiting.list` kind falls in; any kind Today draws and this does not name is "answer". */
export function groupOf(kind: string): WaitingGroup {
	switch (kind) {
		case "preview_missing":
		case "designer_needs_sandbox":
		case "designer_needs_browser":
			return "set_up";
		case "integration":
			return "merge";
		default:
			return "answer";
	}
}
