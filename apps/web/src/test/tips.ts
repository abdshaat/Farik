import { screen } from "@testing-library/react";
import { expect } from "vitest";

/**
 * Every place `text` shows on the page, each of which must sit inside an info button's tip;
 * outside the tips the page does not carry it at all, not even inside a longer paragraph.
 */
export function onlyInTips(text: string) {
	const found = screen.getAllByText(text);
	expect(found.length).toBeGreaterThan(0);
	for (const el of found) expect(el.closest('[role="tooltip"]')).not.toBeNull();
	const shown = document.body.cloneNode(true) as HTMLElement;
	for (const tip of shown.querySelectorAll('[role="tooltip"]')) tip.remove();
	expect(shown.textContent).not.toContain(text);
}
