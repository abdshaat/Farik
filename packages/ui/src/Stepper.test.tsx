import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
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

	it("links_the_earlier_steps", async () => {
		const onPick = vi.fn();
		const { container } = render(
			<Stepper
				steps={["Your project", "What we found", "Your team", "Spending"]}
				current={2}
				onPick={onPick}
			/>,
		);
		const buttons = screen.getAllByRole("button");
		expect(buttons.map((b) => b.textContent?.trim())).toEqual([
			"Your project",
			"What we found",
		]);
		fireEvent.click(screen.getByRole("button", { name: "What we found" }));
		expect(onPick).toHaveBeenCalledWith(1);
		expect(screen.queryByRole("button", { name: "Your team" })).toBeNull();
		expect(screen.queryByRole("button", { name: "Spending" })).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("links_nothing_without_on_pick", () => {
		render(<Stepper steps={["Your project", "What we found"]} current={1} />);
		expect(screen.queryAllByRole("button")).toHaveLength(0);
	});
});
