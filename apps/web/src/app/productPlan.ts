import type { TaskStatus } from "./words.ts";

/** `product_plan.state`, in camelCase. */
export type ProductPlanState = {
	newProject: { name: string } | null;
	approved: boolean;
	task: { taskId: string; status: TaskStatus; createdAt: string } | null;
	documents: { path: string; text: string }[];
};

/** A project's folder name as its owner says it: `little-oak-cakes` is "Little Oak Cakes". */
export function projectName(name: string): string {
	return name
		.split("-")
		.map((word) => word.charAt(0).toUpperCase() + word.slice(1))
		.join(" ");
}
