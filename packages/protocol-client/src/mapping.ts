// The one place wire snake_case and the browser's camelCase meet (rule 6).
// Only keys are mapped; values, including an event's `body.input` and
// `body.output` JSON strings, are left as they are.
const map = (v: unknown, key: (k: string) => string): unknown => {
	if (Array.isArray(v)) return v.map((x) => map(x, key));
	if (v !== null && typeof v === "object") {
		return Object.fromEntries(
			Object.entries(v).map(([k, x]) => [key(k), map(x, key)]),
		);
	}
	return v;
};

export const toCamel = (v: unknown): unknown =>
	map(v, (k) => k.replace(/_([a-z0-9])/g, (_, c: string) => c.toUpperCase()));

export const toSnake = (v: unknown): unknown =>
	map(v, (k) => k.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`));
