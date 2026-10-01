import { defineConfig } from "vite";

export default defineConfig({
	root: "gallery",
	base: "./",
	build: { outDir: "../dist/gallery", emptyOutDir: true },
});
