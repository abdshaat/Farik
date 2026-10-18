import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Segmented } from "./Segmented.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

function Wrapper({ onChange }: { onChange: (v: string) => void }) {
	const [value, setValue] = useState("medium");
	return (
		<Segmented
			name="effort"
			legend="Effort"
			info="More effort thinks longer."
			options={[
				{ value: "low", label: "Low" },
				{ value: "medium", label: "Medium" },
				{ value: "high", label: "High" },
			]}
			value={value}
			onChange={(v) => {
				setValue(v);
				onChange(v);
			}}
		/>
	);
}

describe("Segmented", () => {
	it("is a named group of options picked with a click or the arrow keys", async () => {
		const onChange = vi.fn();
		const { container } = render(<Wrapper onChange={onChange} />);
		const group = screen.getByRole("radiogroup", { name: "Effort" });
		expect(group).toBeTruthy();
		expect((screen.getByLabelText("Medium") as HTMLInputElement).checked).toBe(
			true,
		);
		await userEvent.click(screen.getByLabelText("High"));
		expect(onChange).toHaveBeenLastCalledWith("high");
		await userEvent.keyboard("{ArrowLeft}");
		expect(onChange).toHaveBeenLastCalledWith("medium");
		expect(screen.getByRole("button", { name: /about effort/i })).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
