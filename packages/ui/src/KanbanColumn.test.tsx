import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { KanbanColumn } from "./KanbanColumn.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

describe("KanbanColumn", () => {
	it("names the lane and counts its tasks", async () => {
		const { container } = render(
			<KanbanColumn id="doing" title="Doing" count={3}>
				<p>card</p>
			</KanbanColumn>,
		);
		expect(screen.getByRole("region", { name: "Doing" })).toBeTruthy();
		expect(screen.getByRole("heading").textContent).toContain("3");
		await expectNoAxeViolations(container);
	});
});
