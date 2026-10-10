import { expectNoAxeViolations } from "@catervas/ui/test";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { ShowAdvanced } from "./ShowAdvanced.tsx";

describe("ShowAdvanced", () => {
	it("says show or hide and whether it is open", async () => {
		const onToggle = vi.fn();
		const { container, rerender } = render(
			<ShowAdvanced open={false} onToggle={onToggle} controls="more" />,
		);
		const closed = screen.getByRole("button", { name: en.advancedSwitch });
		expect(closed.getAttribute("aria-expanded")).toBe("false");
		await expectNoAxeViolations(container);
		fireEvent.click(closed);
		expect(onToggle).toHaveBeenCalledTimes(1);

		rerender(
			<>
				<ShowAdvanced open onToggle={onToggle} controls="more" />
				<ul id="more" aria-label={en.advanced} />
			</>,
		);
		const open = screen.getByRole("button", { name: en.advancedHide });
		expect(open.getAttribute("aria-expanded")).toBe("true");
		expect(open.getAttribute("aria-controls")).toBe("more");
		expect(screen.queryByRole("switch")).toBeNull();
		await expectNoAxeViolations(container);
	});
});
