import { en } from "./en.ts";

/** The words for `key`, each `{name}` filled once from `fill`, as text: no `$` pattern is read. */
export function t(
	key: keyof typeof en,
	fill: Record<string, string | number> = {},
): string {
	return en[key].replace(/\{(\w+)\}/g, (all, name: string) =>
		name in fill ? String(fill[name]) : all,
	);
}
