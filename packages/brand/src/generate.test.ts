import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { generateCss, generateTs } from "./generate.ts";
import { readTokens } from "./tokens-schema.ts";

const source = JSON.parse(
	readFileSync(new URL("../tokens/tokens.json", import.meta.url), "utf8"),
);
const tokens = readTokens(source);
const css = generateCss(tokens);
const LIGHT = ':root, [data-theme="light"]';
const DARK = ':root[data-theme="dark"], [data-theme="dark"]';
const block = (selector: string) => {
	const start = css.indexOf(`${selector} {`);
	return css.slice(start, css.indexOf("}", start));
};
const rules = css
	.split("}")
	.map((r) => r.split("{"))
	.filter((p): p is [string, string] => p.length === 2)
	.map(([selector, body]) => ({ selector: selector.trim(), body }));
const ruleFor = (token: string) =>
	rules.find(
		(r) =>
			r.selector.split(/,\s*/).includes(token) &&
			r.body.includes("--farik-color-page"),
	);

describe("generate", () => {
	it("writes every colour token of the light theme under the light selectors", () => {
		const root = block(LIGHT);
		for (const [name, hex] of Object.entries(source.color.light)) {
			expect(root).toContain(`--farik-color-${name}: ${hex};`);
		}
		expect(root).toContain("--farik-color-page: #F3E7D3;");
	});

	it("writes every colour token of the dark theme under the dark selectors", () => {
		const dark = block(DARK);
		for (const [name, hex] of Object.entries(source.color.dark)) {
			expect(dark).toContain(`--farik-color-${name}: ${hex};`);
		}
		expect(dark).toContain("--farik-color-page: #161616;");
	});

	it("writes the dark theme for any element that asks for it", () => {
		const dark = ruleFor('[data-theme="dark"]');
		expect(dark?.body).toContain("--farik-color-page: #161616;");
		const light = ruleFor('[data-theme="light"]');
		expect(light?.body).toContain("--farik-color-page: #F3E7D3;");
	});

	it("a themed element paints its own text and ground", () => {
		const paint = rules.find(
			(r) =>
				r.selector.split(/,\s*/).includes('[data-theme="dark"]') &&
				r.body.includes("background-color"),
		);
		expect(paint?.body).toContain("color: var(--farik-color-ink);");
		expect(paint?.body).toContain("background-color: var(--farik-color-page);");
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
