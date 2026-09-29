import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import sharp from "sharp";
import { ICON_SIZES } from "./assets.ts";

const master = new URL("../assets/logo-mark-1254.png", import.meta.url);
const out = new URL("../assets/icons/", import.meta.url);

// The master already draws its own tile with transparent corners: resize only.
if (import.meta.main) {
	mkdirSync(out, { recursive: true });
	for (const n of ICON_SIZES) {
		await sharp(fileURLToPath(master))
			.resize(n, n, { kernel: "lanczos3" })
			.png()
			.toFile(fileURLToPath(new URL(`icon-${n}.png`, out)));
	}
}
