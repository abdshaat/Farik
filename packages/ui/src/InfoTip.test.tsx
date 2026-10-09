import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { InfoTip } from "./InfoTip.tsx";
import { uiStrings } from "./strings.ts";
import { expectNoAxeViolations } from "./test/axe.ts";

describe("InfoTip", () => {
	it("names its button and describes it with the tip", () => {
		render(
			<InfoTip id="tip" label="About the thing">
				Reads only.
			</InfoTip>,
		);
		const button = screen.getByRole("button", { name: "About the thing" });
		expect(button.getAttribute("aria-describedby")).toBe("tip");
		const tip = document.getElementById("tip");
		expect(tip?.getAttribute("role")).toBe("tooltip");
		expect(tip?.textContent).toBe("Reads only.");
	});

	it("defaults its name to More about this", () => {
		render(<InfoTip id="tip">Text</InfoTip>);
		expect(
			screen.getByRole("button", { name: uiStrings.infoLabel }),
		).toBeTruthy();
		expect(uiStrings.infoLabel).toBe("More about this");
	});

	it("opens on click and closes on a second click or Escape", async () => {
		render(<InfoTip id="tip">Text</InfoTip>);
		const button = screen.getByRole("button", { name: "More about this" });
		expect(button.getAttribute("aria-expanded")).toBe("false");
		await userEvent.click(button);
		expect(button.getAttribute("aria-expanded")).toBe("true");
		await userEvent.click(button);
		expect(button.getAttribute("aria-expanded")).toBe("false");
		await userEvent.click(button);
		expect(button.getAttribute("aria-expanded")).toBe("true");
		await userEvent.keyboard("{Escape}");
		expect(button.getAttribute("aria-expanded")).toBe("false");
	});

	it("has no axe violations open or closed", async () => {
		const { container } = render(<InfoTip id="tip">Text</InfoTip>);
		await expectNoAxeViolations(container);
		await userEvent.click(screen.getByRole("button"));
		await expectNoAxeViolations(container);
	});
});
