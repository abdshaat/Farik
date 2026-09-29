import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { expectNoAxeViolations } from "../src/test/axe.ts";
import { Gallery } from "./Gallery.tsx";

const NAMES = [
	"Avatar",
	"Button",
	"ChatList",
	"Choice",
	"Dialog",
	"DiffView",
	"KanbanColumn",
	"List",
	"RoleTag",
	"StatusWord",
	"Stepper",
	"Switch",
	"Table",
	"TextArea",
	"TextField",
];

describe("Gallery", () => {
	it("shows every component in both themes", () => {
		const { container } = render(<Gallery />);
		for (const theme of ["light", "dark"]) {
			const column = container.querySelectorAll(`[data-theme="${theme}"]`);
			expect(column.length).toBe(1);
			for (const name of NAMES) {
				expect(
					column[0]?.querySelectorAll(`[data-component="${name}"]`).length,
				).toBe(1);
			}
		}
		expect(container.querySelectorAll("dialog[open]").length).toBe(0);
	});

	it("passes the accessibility check", async () => {
		const { container } = render(<Gallery />);
		await expectNoAxeViolations(container);
	});
});
