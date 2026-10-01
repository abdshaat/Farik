import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Switch } from "./Switch.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

describe("Switch", () => {
	it("switches on and off", async () => {
		const onChange = vi.fn();
		const { container, rerender } = render(
			<Switch id="s" label="Notify me" checked={false} onChange={onChange} />,
		);
		const control = screen.getByRole("switch", { name: "Notify me" });
		expect(control.getAttribute("aria-checked")).toBe("false");
		control.focus();
		await userEvent.keyboard(" ");
		expect(onChange).toHaveBeenLastCalledWith(true);
		rerender(
			<Switch id="s" label="Notify me" checked={true} onChange={onChange} />,
		);
		expect(control.getAttribute("aria-checked")).toBe("true");
		await userEvent.keyboard(" ");
		expect(onChange).toHaveBeenLastCalledWith(false);
		await expectNoAxeViolations(container);
	});
});
