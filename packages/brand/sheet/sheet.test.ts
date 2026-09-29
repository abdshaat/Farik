// @vitest-environment jsdom
import axe from "axe-core";
import { beforeEach, describe, expect, it } from "vitest";
import {
	AVATAR_KEYS,
	contrastRatio,
	ICON_SIZES,
	TEXT_PAIRS,
	tokens,
} from "../src/index.ts";
import { renderSheet } from "./sheet.ts";

const THEMES = ["light", "dark"] as const;
const kebab = (s: string) => s.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
const camel = (s: string) => s.replace(/-(.)/g, (_, c) => c.toUpperCase());
const hex = (theme: (typeof THEMES)[number], name: string) =>
	(tokens.color[theme] as Record<string, string>)[camel(name)] as string;

let root: HTMLElement;
beforeEach(() => {
	document.body.innerHTML = '<main id="root"></main>';
	root = document.getElementById("root") as HTMLElement;
	renderSheet(root);
});

describe("brand sheet", () => {
	it("shows a swatch for every colour in both themes", () => {
		for (const theme of THEMES) {
			for (const [key, value] of Object.entries(tokens.color[theme])) {
				const name = kebab(key);
				const found = root.querySelectorAll(`[data-swatch="${theme}:${name}"]`);
				expect(found).toHaveLength(1);
				expect(found[0]?.textContent).toContain(name);
				expect(found[0]?.textContent).toContain(value.toUpperCase());
			}
		}
	});

	it("shows every checked pair with its ratio", () => {
		expect(root.querySelectorAll("[data-pair]")).toHaveLength(
			TEXT_PAIRS.length * THEMES.length,
		);
		for (const theme of THEMES) {
			for (const p of TEXT_PAIRS) {
				const found = root.querySelectorAll(
					`[data-pair="${theme}:${p.foreground}/${p.background}"]`,
				);
				expect(found).toHaveLength(1);
				const ratio = contrastRatio(
					hex(theme, p.foreground),
					hex(theme, p.background),
				).toFixed(2);
				expect(found[0]?.textContent).toContain(ratio);
				expect(found[0]?.textContent).toContain(String(p.minimum));
			}
		}
	});

	it("shows every icon and avatar", () => {
		for (const n of ICON_SIZES) {
			const img = root.querySelector(`img[alt="Icon, ${n} px"]`);
			expect(img).not.toBeNull();
		}
		for (const key of AVATAR_KEYS) {
			expect(root.querySelector(`img[data-avatar="${key}"]`)).not.toBeNull();
		}
		const imgs = [...root.querySelectorAll("img")];
		expect(imgs.length).toBeGreaterThanOrEqual(
			ICON_SIZES.length + AVATAR_KEYS.length,
		);
		for (const img of imgs) {
			expect(img.getAttribute("alt")?.trim()).toBeTruthy();
		}
	});

	it("has no axe violations", async () => {
		const { violations } = await axe.run(root);
		expect(violations).toEqual([]);
	});
});
