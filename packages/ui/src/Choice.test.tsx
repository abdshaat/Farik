import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { Choice } from "./Choice.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

const options = [
	{ value: "one", label: "One" },
	{ value: "two", label: "Two", description: "The middle one" },
	{ value: "three", label: "Three" },
];

function Wrapper({ onChange }: { onChange: (v: string) => void }) {
	const [value, setValue] = useState("one");
	return (
		<Choice
			name="pick"
			legend="Pick one"
			options={options}
			value={value}
			onChange={(v) => {
				setValue(v);
				onChange(v);
			}}
		/>
	);
}

describe("Choice", () => {
	it("picks an option with a click or the arrow keys", async () => {
		const onChange = vi.fn();
		const { container } = render(<Wrapper onChange={onChange} />);
		await userEvent.click(screen.getByLabelText(/Two/));
		expect(onChange).toHaveBeenLastCalledWith("two");
		await userEvent.keyboard("{ArrowDown}");
		expect(onChange).toHaveBeenLastCalledWith("three");
		expect((screen.getByLabelText(/Three/) as HTMLInputElement).checked).toBe(
			true,
		);
		await expectNoAxeViolations(container);
	});

	it("names the group", async () => {
		const { container } = render(<Wrapper onChange={() => {}} />);
		expect(screen.getByRole("group", { name: "Pick one" })).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
