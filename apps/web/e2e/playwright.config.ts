import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineConfig, devices } from "@playwright/test";

// One worker: each test starts its own farik, and the journey is short.
export default defineConfig({
	testDir: ".",
	workers: 1,
	forbidOnly: true,
	reporter: "list",
	// Failure traces go outside the repository, where neither git nor biome sees them.
	outputDir: join(tmpdir(), "farik-e2e-results"),
	use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } },
});
