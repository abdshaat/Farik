import { render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { TextArea } from "./TextArea.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

function Wrapper() {
	const [value, setValue] = useState("");
	return (
		<TextArea id="brief" label="Brief" value={value} onChange={setValue} />
	);
}

describe("TextArea", () => {
	it("labels the area and keeps line breaks", async () => {
		const { container } = render(<Wrapper />);
		const area = screen.getByLabelText("Brief");
		expect(area.tagName).toBe("TEXTAREA");
		await userEvent.type(area, "a{Enter}b");
		expect((area as HTMLTextAreaElement).value).toBe("a\nb");
		await expectNoAxeViolations(container);
	});
});
