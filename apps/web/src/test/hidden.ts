// What a test asks of a text that the agent or a seller wrote with a hidden character in it.

import { expect } from "vitest";

/**
 * `element` shows the reordering character written out, never as itself. The words in a field the
 * owner edits are left out: they are the owner's to change, and the page reads them as typed.
 */
export function showsWhatItHides(element: Element) {
	const read = element.cloneNode(true) as Element;
	for (const field of read.querySelectorAll("textarea")) field.remove();
	const text = read.textContent ?? "";
	expect(text).not.toContain("‮");
	expect(text).toContain("\\u{202e}");
}
