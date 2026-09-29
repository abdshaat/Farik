import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { generateCss, generateTs } from "./generate.ts";
import { readTokens } from "./tokens-schema.ts";

const source = JSON.parse(
	readFileSync(new URL("../tokens/tokens.json", import.meta.url), "utf8"),
);
const tokens = readTokens(source);
const css = generateCss(tokens);
const block = (selector: string) => {
	const start = css.indexOf(`${selector} {`);
	return css.slice(start, css.indexOf("}", start));
};

describe("generate", () => {
	it("writes every colour token of the light theme under :root", () => {
		const root = block(":root");
		for (const [name, hex] of Object.entries(source.color.light)) {
			expect(root).toContain(`--farik-color-${name}: ${hex};`);
		}
		expect(root).toContain("--farik-color-page: #F3E7D3;");
	});

	it('writes every colour token of the dark theme under :root[data-theme="dark"]', () => {
		const dark = block(':root[data-theme="dark"]');
		for (const [name, hex] of Object.entries(source.color.dark)) {
			expect(dark).toContain(`--farik-color-${name}: ${hex};`);
		}
		expect(dark).toContain("--farik-color-page: #161616;");
	});

	it("writes each type step as size, line height, weight and family", () => {
		for (const line of [
			"--farik-type-body-size: 16px;",
			"--farik-type-body-line-height: 24px;",
			"--farik-type-body-weight: 400;",
			"--farik-type-title-weight: 600;",
			"--farik-type-body-family: 'Space Grotesk', system-ui, sans-serif;",
			"--farik-type-display-family: 'Silkscreen', monospace;",
			"--farik-type-code-family: 'JetBrains Mono', ui-monospace, monospace;",
		]) {
			expect(css).toContain(line);
		}
	});

	it("refuses a colour defined in one theme only", () => {
		const { link: _link, ...dark } = source.color.dark;
		const bad = { ...source, color: { light: source.color.light, dark } };
		expect(() => readTokens(bad)).toThrow(/link.*dark|dark.*link/);
	});

	it("refuses a colour defined in the light theme only", () => {
		const { link: _link, ...light } = source.color.light;
		const bad = { ...source, color: { light, dark: source.color.dark } };
		expect(() => readTokens(bad)).toThrow(/link.*light|light.*link/);
	});

	it("refuses a colour that is not #rrggbb", () => {
		const bad = {
			...source,
			color: {
				...source.color,
				light: { ...source.color.light, ink: "black" },
			},
		};
		expect(() => readTokens(bad)).toThrow(/ink/);
		const lower = {
			...source,
			color: {
				...source.color,
				light: { ...source.color.light, ink: "#f3e7d3" },
			},
		};
		expect(() => readTokens(lower)).toThrow(/ink/);
	});

	it("generates camelCase keys in the TypeScript module", () => {
		const ts = generateTs(tokens);
		expect(ts).toContain("inkMuted");
		expect(ts).toContain("statusWaiting");
		expect(ts).not.toContain("ink-muted");
		expect(ts.trimEnd().endsWith("as const;")).toBe(true);
	});
});
