import { rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
	FullResult,
	Reporter,
	TestCase,
	TestResult,
} from "@playwright/test/reporter";

/** What `startServe`'s `stop()` attaches: the temporary folders its serve made. */
export const FOLDERS = "catervas-e2e-folders";

const mine = join(tmpdir(), "catervas-e2e-");

const folders = (result: TestResult): string[] =>
	result.attachments
		.filter((one) => one.name === FOLDERS && one.body)
		.flatMap((one) => JSON.parse(one.body?.toString() ?? "[]") as string[])
		.filter((folder) => folder.startsWith(mine));

/**
 * Removes a passing test's folders once its server has exited, and the shared state folder once the
 * whole run passed. A failing test keeps its folder: its event log is what explains the failure.
 */
export default class Cleanup implements Reporter {
	private shared = new Set<string>();

	onTestEnd(_test: TestCase, result: TestResult): void {
		const found = folders(result);
		// The last of a test's folders is the state folder every serve of the worker shares.
		for (const folder of found.filter((one) => one.includes("-config-")))
			this.shared.add(folder);
		if (result.status !== "passed") return;
		for (const folder of found.filter((one) => !one.includes("-config-")))
			rmSync(folder, { recursive: true, force: true });
	}

	onEnd(result: FullResult): void {
		if (result.status !== "passed") return;
		for (const folder of this.shared)
			rmSync(folder, { recursive: true, force: true });
	}
}
