import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { readTokens, type ThemeName, type Tokens } from "./tokens-schema.ts";

const FAMILY = {
	pixel: "'Silkscreen', monospace",
	sans: "'Space Grotesk', system-ui, sans-serif",
	mono: "'JetBrains Mono', ui-monospace, monospace",
} as const;

const camel = (s: string) =>
	s.replace(/-([a-z0-9])/g, (_, c: string) => c.toUpperCase());

export function generateCss(tokens: Tokens): string {
	const block = (selector: string, lines: string[]) =>
		`${selector} {\n${lines.map((l) => `\t${l}\n`).join("")}}\n`;
	const shared: string[] = [];
	for (const [name, s] of Object.entries(tokens.type)) {
		shared.push(
			`--farik-type-${name}-size: ${s.size}px;`,
			`--farik-type-${name}-line-height: ${s.lineHeight}px;`,
			`--farik-type-${name}-weight: ${s.weight};`,
			`--farik-type-${name}-family: ${FAMILY[s.family]};`,
		);
	}
	for (const [k, n] of Object.entries(tokens.space))
		shared.push(`--farik-space-${k}: ${n}px;`);
	for (const [k, n] of Object.entries(tokens.radius))
		shared.push(`--farik-radius-${k}: ${n}px;`);
	const theme = (t: ThemeName) =>
		Object.entries(tokens.color[t]).map(
			([k, hex]) => `--farik-color-${k}: ${hex};`,
		);
	return `${block(":root", [...theme("light"), ...shared])}\n${block(':root[data-theme="dark"]', theme("dark"))}`;
}

export function generateTs(tokens: Tokens): string {
	const keyed = <T>(o: Record<string, T>) =>
		Object.fromEntries(Object.entries(o).map(([k, v]) => [camel(k), v]));
	const value = {
		color: { light: keyed(tokens.color.light), dark: keyed(tokens.color.dark) },
		type: keyed(tokens.type),
		space: tokens.space,
		radius: keyed(tokens.radius),
	};
	return `export type ThemeName = "light" | "dark";

export const tokens = ${JSON.stringify(value, null, "\t")} as const;
`;
}

if (import.meta.main) {
	const tokens = readTokens(
		JSON.parse(
			readFileSync(new URL("../tokens/tokens.json", import.meta.url), "utf8"),
		),
	);
	const write = (rel: string, text: string) => {
		const url = new URL(rel, import.meta.url);
		mkdirSync(new URL(".", url), { recursive: true });
		writeFileSync(url, text);
	};
	write("../dist/tokens.css", generateCss(tokens));
	write("./generated/tokens.ts", generateTs(tokens));
}
