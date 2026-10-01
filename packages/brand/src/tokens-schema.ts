export type ThemeName = "light" | "dark";
export type TypeStep = {
	size: number;
	lineHeight: number;
	weight: 400 | 600;
	family: "pixel" | "sans" | "mono";
};
export type Tokens = {
	color: Record<ThemeName, Record<string, string>>;
	type: Record<string, TypeStep>;
	space: Record<string, number>;
	radius: Record<string, number>;
};

const isRecord = (v: unknown): v is Record<string, unknown> =>
	typeof v === "object" && v !== null && !Array.isArray(v);

function record(v: unknown, where: string): Record<string, unknown> {
	if (!isRecord(v)) throw new Error(`${where} must be an object`);
	return v;
}

function numbers(v: unknown, where: string): Record<string, number> {
	const out: Record<string, number> = {};
	for (const [k, n] of Object.entries(record(v, where))) {
		if (typeof n !== "number")
			throw new Error(`${where}.${k} must be a number`);
		out[k] = n;
	}
	return out;
}

function colors(v: unknown, theme: string): Record<string, string> {
	const out: Record<string, string> = {};
	for (const [k, hex] of Object.entries(record(v, `color.${theme}`))) {
		if (typeof hex !== "string" || !/^#[0-9A-F]{6}$/.test(hex)) {
			throw new Error(
				`color.${theme}.${k} must be #RRGGBB in upper case, got ${String(hex)}`,
			);
		}
		out[k] = hex;
	}
	return out;
}

/** Maps tokens.json (snake_case) to the TypeScript shape (camelCase); throws on the first bad or one-sided token. */
export function readTokens(json: unknown): Tokens {
	const root = record(json, "tokens");
	const color = record(root.color, "color");
	const light = colors(color.light, "light");
	const dark = colors(color.dark, "dark");
	for (const [have, other, name] of [
		[light, dark, "dark"],
		[dark, light, "light"],
	] as const) {
		for (const k of Object.keys(have)) {
			if (!(k in other))
				throw new Error(`color ${k} is missing from the ${name} theme`);
		}
	}
	const type: Record<string, TypeStep> = {};
	for (const [k, raw] of Object.entries(record(root.type, "type"))) {
		const s = record(raw, `type.${k}`);
		const { size, line_height: lineHeight, weight, family } = s;
		if (typeof size !== "number" || typeof lineHeight !== "number") {
			throw new Error(`type.${k} needs a numeric size and line_height`);
		}
		if (weight !== 400 && weight !== 600)
			throw new Error(`type.${k}.weight must be 400 or 600`);
		if (family !== "pixel" && family !== "sans" && family !== "mono") {
			throw new Error(`type.${k}.family must be pixel, sans or mono`);
		}
		type[k] = { size, lineHeight, weight, family };
	}
	return {
		color: { light, dark },
		type,
		space: numbers(root.space, "space"),
		radius: numbers(root.radius, "radius"),
	};
}
