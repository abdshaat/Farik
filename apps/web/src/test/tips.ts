import { screen } from "@testing-library/react";
import { expect } from "vitest";

/** Every place `text` shows on the page, each of which must sit inside an info button's tip. */
export function onlyInTips(text: string) {
	const found = screen.getAllByText(text);
	expect(found.length).toBeGreaterThan(0);
	for (const el of found) expect(el.closest('[role="tooltip"]')).not.toBeNull();
}
