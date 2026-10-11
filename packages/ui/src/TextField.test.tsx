import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { TextField } from "./TextField.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

function Wrapper({ onChange }: { onChange: (v: string) => void }) {
	const [value, setValue] = useState("");
	return (
		<TextField
			id="name"
			label="Team name"
			value={value}
			onChange={(v) => {
				setValue(v);
				onChange(v);
			}}
		/>
	);
}

describe("TextField", () => {
	it("labels the field and reports changes", async () => {
		const onChange = vi.fn();
		const { container } = render(<Wrapper onChange={onChange} />);
		await userEvent.type(screen.getByLabelText("Team name"), "abc");
		expect(onChange.mock.calls.map((c) => c[0])).toEqual(["a", "ab", "abc"]);
		await expectNoAxeViolations(container);
	});

	it("says what is wrong and marks the field invalid", async () => {
		const { container } = render(
			<TextField
				id="name"
				label="Team name"
				value=""
				onChange={() => {}}
				hint="Shown to your team"
				error="Give the team a name"
				required
			/>,
		);
		const input = screen.getByLabelText(/Team name/);
		expect(input.getAttribute("aria-invalid")).toBe("true");
		const ids = (input.getAttribute("aria-describedby") ?? "").split(" ");
		const texts = ids.map((id) => document.getElementById(id)?.textContent);
		expect(texts).toContain("Give the team a name");
		expect(texts).toContain("Shown to your team");
		await expectNoAxeViolations(container);
	});

	it("puts its info behind a button beside the label", async () => {
		const { container } = render(
			<TextField
				id="port"
				label="Port"
				value=""
				onChange={() => {}}
				info="Detail."
			/>,
		);
		const button = screen.getByRole("button", { name: "More about this" });
		expect(screen.getByText("Port").contains(button)).toBe(false);
		const tip = container.querySelector("#port-info");
		expect(tip?.getAttribute("role")).toBe("tooltip");
		expect(tip?.textContent).toBe("Detail.");
		const input = screen.getByRole("textbox", { name: "Port" });
		expect(input.getAttribute("aria-describedby")).toContain("port-info");
		await expectNoAxeViolations(container);
	});

	it("has no info button without info", () => {
		render(<TextField id="port" label="Port" value="" onChange={() => {}} />);
		expect(screen.queryByRole("button")).toBeNull();
	});
});
