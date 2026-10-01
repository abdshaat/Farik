import { defineConfig } from "vite";

// The sheet's own build; Vitest reads ../vitest.config.ts instead.
export default defineConfig({
	root: import.meta.dirname,
	base: "./",
	build: { outDir: "../dist/sheet", emptyOutDir: true },
});
