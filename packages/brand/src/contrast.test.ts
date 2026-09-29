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
		expect(contrastRatio("#A44D2B", "#F3E7D3").toFixed(2)).toBe("4.68");
		expect(contrastRatio("#161616", "#D8896A").toFixed(2)).toBe("6.64");
		expect(contrastRatio("#F3E7D3", "#161616").toFixed(2)).toBe("14.81");
		expect(contrastRatio("#5A8DFF", "#F3E7D3").toFixed(2)).toBe("2.57");
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
});
