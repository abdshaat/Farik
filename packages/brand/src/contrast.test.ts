import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { contrastRatio, TEXT_PAIRS } from "./contrast.ts";
import { readTokens, type ThemeName } from "./tokens-schema.ts";

const tokens = readTokens(
	JSON.parse(
		readFileSync(new URL("../tokens/tokens.json", import.meta.url), "utf8"),
	),
);

describe("contrast", () => {
	it("matches the ratios brand.md records", () => {
		expect(contrastRatio("#96533A", "#F3E7D3").toFixed(2)).toBe("4.78");
		expect(contrastRatio("#161616", "#D8896A").toFixed(2)).toBe("6.64");
		expect(contrastRatio("#F3E7D3", "#161616").toFixed(2)).toBe("14.81");
		expect(contrastRatio("#5F7A9B", "#F3E7D3").toFixed(2)).toBe("3.62");
	});

	it("is the same either way round", () => {
		expect(contrastRatio("#161616", "#F3E7D3")).toBe(
			contrastRatio("#F3E7D3", "#161616"),
		);
	});

	for (const theme of ["light", "dark"] as ThemeName[]) {
		it(`holds every pair to its minimum in the ${theme} theme`, () => {
			const c = tokens.color[theme];
			for (const p of TEXT_PAIRS) {
				const fg = c[p.foreground];
				const bg = c[p.background];
				if (fg === undefined || bg === undefined)
					throw new Error(
						`${p.foreground}/${p.background} names a missing token`,
					);
				const ratio = contrastRatio(fg, bg);
				expect(
					ratio,
					`${p.foreground} on ${p.background} in ${theme}: ${ratio.toFixed(2)}, needs ${p.minimum}`,
				).toBeGreaterThanOrEqual(p.minimum);
			}
		});
	}

	it("lists every role colour against role-ink", () => {
		const roles = Object.keys(tokens.color.light).filter(
			(k) => k.startsWith("role-") && k !== "role-ink",
		);
		expect(roles.length).toBeGreaterThan(0);
		for (const role of roles) {
			expect(
				TEXT_PAIRS.some(
					(p) => p.foreground === "role-ink" && p.background === role,
				),
				`role-ink on ${role}`,
			).toBe(true);
		}
	});
	for (const theme of ["light", "dark"] as ThemeName[]) {
		it(`gives every job its own colour in the ${theme} theme`, () => {
			const c = tokens.color[theme];
			const jobs = [
				"action",
				"role-product-manager",
				"role-scrum-master",
				"role-architect",
				"role-developer",
				"role-marketing-specialist",
				"status-done",
				"status-working",
				"status-waiting",
				"focus",
			];
			const seen = new Map<string, string>();
			for (const job of jobs) {
				const value = c[job]?.toLowerCase() ?? "";
				const other = seen.get(value);
				expect(
					other,
					`${job} and ${other} share ${value} in ${theme}`,
				).toBeUndefined();
				seen.set(value, job);
			}
		});
	}

	it("pins the pairs the plan lists", () => {
		expect(
			TEXT_PAIRS.map((p) => [p.foreground, p.background, p.minimum]),
		).toEqual([
			["ink", "page", 4.5],
			["ink-muted", "page", 4.5],
			["link", "page", 4.5],
			["status-done", "page", 4.5],
			["status-working", "page", 4.5],
			["status-waiting", "page", 4.5],
			["control-border", "page", 3],
			["focus", "page", 3],
			["ink", "surface", 4.5],
			["ink-muted", "surface", 4.5],
			["link", "surface", 4.5],
			["status-done", "surface", 4.5],
			["status-working", "surface", 4.5],
			["status-waiting", "surface", 4.5],
			["control-border", "surface", 3],
			["focus", "surface", 3],
			["action-ink", "action", 4.5],
			["band-ink", "band", 4.5],
			["focus", "band", 3],
			["role-ink", "role-product-manager", 4.5],
			["role-ink", "role-scrum-master", 4.5],
			["role-ink", "role-architect", 4.5],
			["role-ink", "role-developer", 4.5],
			["role-ink", "role-marketing-specialist", 4.5],
		]);
	});
});
