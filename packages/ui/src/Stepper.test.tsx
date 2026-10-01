import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Stepper } from "./Stepper.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

describe("Stepper", () => {
	it("marks the current step", async () => {
		const { container } = render(
			<Stepper
				steps={["Your project", "What we found", "Your team"]}
				current={1}
			/>,
		);
		const items = screen.getAllByRole("listitem");
		expect(items).toHaveLength(3);
		expect(items.map((i) => i.getAttribute("aria-current"))).toEqual([
			null,
			"step",
			null,
		]);
		expect(screen.getByText("Step 2 of 3")).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
