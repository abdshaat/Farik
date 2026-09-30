import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import styles from "./StatusWord.module.css";
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

	it("draws a pale pill round a done word when asked", async () => {
		const { container } = render(
			<>
				<StatusWord tone="done" pill>
					Ready
				</StatusWord>
				<StatusWord tone="done">Accepted</StatusWord>
			</>,
		);
		expect(styles.pill).toBeTruthy();
		expect(screen.getByText("Ready").classList).toContain(styles.pill);
		expect(screen.getByText("Accepted").classList).not.toContain(styles.pill);
		await expectNoAxeViolations(container);
	});
});
