import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Button } from "./Button.tsx";
import { uiStrings } from "./strings.ts";
import { expectNoAxeViolations } from "./test/axe.ts";

describe("Button", () => {
	it("renders a primary button that does what it says", async () => {
		const onClick = vi.fn();
		const { container } = render(
			<Button kind="primary" onClick={onClick}>
				Save
			</Button>,
		);
		await userEvent.click(screen.getByRole("button", { name: "Save" }));
		expect(onClick).toHaveBeenCalledTimes(1);
		await expectNoAxeViolations(container);
	});

	it("a busy button cannot be pressed twice", async () => {
		const onClick = vi.fn();
		const { container } = render(
			<Button busy onClick={onClick}>
				Save
			</Button>,
		);
		const button = screen.getByRole("button", {
			name: `Save ${uiStrings.busy}`,
		});
		// The busy word is for screen readers; the visible label stays "Save".
		expect(screen.getByText(uiStrings.busy).className).toMatch(/hidden/);
		expect((button as HTMLButtonElement).disabled).toBe(true);
		expect(button.getAttribute("aria-busy")).toBe("true");
		await userEvent.click(button);
		expect(onClick).not.toHaveBeenCalled();
		await expectNoAxeViolations(container);
	});
});
