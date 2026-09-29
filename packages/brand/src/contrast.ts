/** WCAG 2.x contrast ratio of two #RRGGBB colours, 1 to 21. */
export function contrastRatio(foreground: string, background: string): number {
	const [a, b] = [luminance(foreground), luminance(background)];
	return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

function luminance(hex: string): number {
	const [r, g, b] = [1, 3, 5].map((i) => {
		const c = Number.parseInt(hex.slice(i, i + 2), 16) / 255;
		return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
	}) as [number, number, number];
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export type TextPair = {
	foreground: string;
	background: string;
	minimum: 4.5 | 3;
};

const pair = (
	foreground: string,
	background: string,
	minimum: 4.5 | 3,
): TextPair => ({ foreground, background, minimum });

const ROLES = [
	"product-manager",
	"scrum-master",
	"architect",
	"developer",
	"marketing-specialist",
];

/** Token names from tokens.json; text needs 4.5, controls 3 (WCAG 2.2 AA). */
export const TEXT_PAIRS: readonly TextPair[] = [
	...["page", "surface"].flatMap((bg) => [
		...[
			"ink",
			"ink-muted",
			"link",
			"status-done",
			"status-working",
			"status-waiting",
		].map((fg) => pair(fg, bg, 4.5)),
		pair("control-border", bg, 3),
		pair("focus", bg, 3),
	]),
	pair("action-ink", "action", 4.5),
	pair("band-ink", "band", 4.5),
	pair("focus", "band", 3),
	...ROLES.map((r) => pair("role-ink", `role-${r}`, 4.5)),
	pair("ink", "diff-added", 4.5),
	pair("ink", "diff-removed", 4.5),
];
