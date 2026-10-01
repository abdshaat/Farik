import { defineConfig } from "vitest/config";

export default defineConfig({
	test: {
		environment: "jsdom",
		setupFiles: ["src/test/setup.ts"],
		include: ["src/**/*.test.{ts,tsx}"],
		// Half the cores: jsdom files are heavy, and a full pool on a machine already
		// busy with cargo builds stretched cold first tests past their 5 s timeout.
		maxWorkers: "50%",
	},
});
