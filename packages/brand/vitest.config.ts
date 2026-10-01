import { defineConfig } from "vitest/config";

export default defineConfig({
	test: {
		include: ["src/**/*.test.ts", "sheet/**/*.test.ts"],
		environment: "node",
	},
});
