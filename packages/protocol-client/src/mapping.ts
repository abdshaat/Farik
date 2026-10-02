// The one place wire snake_case and the browser's camelCase meet (rule 6).
// Only keys are mapped; values, including an event's `body.input` and
// `body.output` JSON strings, are left as they are. So are the names inside a
// connector's `tools`, `keys`, `tags` and `headers` maps, which the user or the
// server chose (`list_bases`, `API_KEY`, `Authorization`), not the wire. So are the paths of a
// skill's `files` (`SKILL.md`, `references/api_notes.md`).
const NAMED = new Set(["tools", "keys", "tags", "headers", "files"]);
const map = (v: unknown, key: (k: string) => string): unknown => {
	if (Array.isArray(v)) return v.map((x) => map(x, key));
	if (v !== null && typeof v === "object") {
		return Object.fromEntries(
			Object.entries(v).map(([k, x]) => [
				key(k),
				NAMED.has(k) && x !== null && typeof x === "object" && !Array.isArray(x)
					? x
					: map(x, key),
			]),
		);
	}
	return v;
};

export const toCamel = (v: unknown): unknown =>
	map(v, (k) => k.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase()));

export const toSnake = (v: unknown): unknown =>
	map(v, (k) => k.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`));
