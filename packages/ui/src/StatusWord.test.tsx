import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { StatusWord } from "./StatusWord.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

describe("StatusWord", () => {
	it("says the status in words", async () => {
		const tones = ["done", "working", "waiting"] as const;
		const { container } = render(
			tones.map((tone) => (
				<StatusWord key={tone} tone={tone}>
					{`is ${tone}`}
				</StatusWord>
			)),
		);
		const classes = tones.map(
			(tone) => screen.getByText(`is ${tone}`).className,
		);
		expect(new Set(classes).size).toBe(3);
		await expectNoAxeViolations(container);
	});
});
