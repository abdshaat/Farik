import axe from "axe-core";
import { expect } from "vitest";

// color-contrast is off: jsdom does not lay out, and the brand's contrast test holds colour.
export async function expectNoAxeViolations(container: Element): Promise<void> {
	const { violations } = await axe.run(container, {
		rules: { "color-contrast": { enabled: false } },
	});
	expect(
		violations.map(
			(v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
		),
	).toEqual([]);
}
