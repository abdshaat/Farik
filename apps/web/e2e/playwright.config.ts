import { createHash } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineConfig, devices } from "@playwright/test";

// One worker: each test starts its own farik, and the journey is short.
export default defineConfig({
	testDir: ".",
	workers: 1,
	forbidOnly: true,
	reporter: [["list"], ["./fixtures/cleanup-reporter.ts"]],
	// Failure traces go outside the repository, where neither git nor biome sees them, in a folder
	// per worktree: Playwright empties it when it starts, so two checks must not share one.
	outputDir: join(
		tmpdir(),
		`farik-e2e-results-${createHash("sha256")
			.update(import.meta.dirname)
			.digest("hex")
			.slice(0, 12)}`,
	),
	use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } },
});
