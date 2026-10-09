import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
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

	it("draws its mark as an svg, not a glyph", () => {
		render(<InfoTip id="tip">Text</InfoTip>);
		const button = screen.getByRole("button");
		expect(button.querySelector("svg")).not.toBeNull();
		expect(button.textContent).toBe("");
	});

	it("opens on keyboard focus; Escape closes it while focus stays; tabbing away closes it", async () => {
		render(
			<>
				<InfoTip id="tip">Text</InfoTip>
				<button type="button">Next</button>
			</>,
		);
		const button = screen.getByRole("button", { name: "More about this" });
		const tip = document.getElementById("tip");
		await userEvent.tab();
		expect(document.activeElement).toBe(button);
		expect(button.getAttribute("aria-expanded")).toBe("true");
		expect(tip?.hasAttribute("data-open")).toBe(true);
		await userEvent.keyboard("{Escape}");
		expect(button.getAttribute("aria-expanded")).toBe("false");
		expect(tip?.hasAttribute("data-open")).toBe(false);
		expect(document.activeElement).toBe(button);
		await userEvent.tab();
		await userEvent.tab({ shift: true });
		expect(button.getAttribute("aria-expanded")).toBe("true");
		await userEvent.tab();
		expect(button.getAttribute("aria-expanded")).toBe("false");
		expect(tip?.hasAttribute("data-open")).toBe(false);
	});

	it("shifts its tip left to stay on a narrow screen", async () => {
		vi.stubGlobal("innerWidth", 360);
		render(<InfoTip id="tip">Text</InfoTip>);
		const tip = document.getElementById("tip") as HTMLElement;
		vi.spyOn(tip, "getBoundingClientRect").mockReturnValue({
			left: 300,
			right: 618,
			top: 0,
			bottom: 0,
			width: 318,
			height: 0,
			x: 300,
			y: 0,
			toJSON: () => ({}),
		});
		await userEvent.click(screen.getByRole("button"));
		expect(tip.style.translate).toBe("-274px");
		await userEvent.click(screen.getByRole("button"));
		expect(tip.style.translate).toBe("");
		vi.unstubAllGlobals();
	});

	it("has no axe violations open or closed", async () => {
		const { container } = render(<InfoTip id="tip">Text</InfoTip>);
		await expectNoAxeViolations(container);
		await userEvent.click(screen.getByRole("button"));
		await expectNoAxeViolations(container);
	});
});
