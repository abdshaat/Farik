import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { AVATAR_KEYS, ICON_SIZES } from "./assets.ts";
import { readTokens } from "./tokens-schema.ts";

const here = (path: string) => new URL(`../${path}`, import.meta.url);
const sha = (url: URL) =>
	createHash("sha256").update(readFileSync(url)).digest("hex");
const size = (url: URL) => {
	const png = readFileSync(url);
	return [png.readUInt32BE(16), png.readUInt32BE(20)];
};

describe("assets", () => {
	it("has every icon size at its size", () => {
		for (const n of ICON_SIZES) {
			expect(size(here(`assets/icons/icon-${n}.png`))).toEqual([n, n]);
		}
	});

	it("has a 256 px avatar for every key", () => {
		expect(AVATAR_KEYS).toHaveLength(10);
		for (const key of AVATAR_KEYS) {
			expect(size(here(`assets/avatars/${key}-256.png`))).toEqual([256, 256]);
		}
	});

	it("keeps the founder's masters unchanged", () => {
		const master = (name: string) =>
			new URL(`../../../docs/brand/assets/${name}`, import.meta.url);
		expect(sha(here("assets/logo-mark-1254.png"))).toBe(
			sha(master("logo-mark.png")),
		);
		expect(sha(here("assets/wordmark-1024.png"))).toBe(
			sha(master("wordmark.png")),
		);
	});

	it("bundles every font family the type tokens name", () => {
		const tokens = readTokens(
			JSON.parse(readFileSync(here("tokens/tokens.json"), "utf8")),
		);
		const css = readFileSync(here("src/fonts.css"), "utf8");
		const pkg = {
			pixel: "silkscreen",
			sans: "space-grotesk",
			mono: "jetbrains-mono",
		};
		for (const step of Object.values(tokens.type)) {
			expect(css).toContain(`@import "@fontsource/${pkg[step.family]}/`);
		}
	});
});
