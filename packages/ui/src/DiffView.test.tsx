import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DiffView } from "./DiffView.tsx";
import { uiStrings } from "./strings.ts";
import { expectNoAxeViolations } from "./test/axe.ts";

const diff = "--- a/f.txt\n+++ b/f.txt\n@@ -1,2 +1,2 @@\n same\n-old\n+new";

describe("DiffView", () => {
	it("shows added and removed lines with their marks", async () => {
		const { container } = render(<DiffView diff={diff} label="Changes" />);
		expect(screen.getByRole("region", { name: "Changes" })).toBeTruthy();
		expect(screen.getByText("f.txt")).toBeTruthy();
		const added = screen.getByText("+new").closest("div");
		const removed = screen.getByText("\u2212old").closest("div");
		expect(added?.textContent).toBe(`+new${uiStrings.added}`);
		expect(removed?.textContent).toBe(`−old${uiStrings.removed}`);
		expect(screen.getByText(/same/).closest("div")?.textContent).toBe(" same");
		await expectNoAxeViolations(container);
	});

	it("says when there are no changes", async () => {
		const { container } = render(<DiffView diff="" label="Changes" />);
		expect(screen.getByText(uiStrings.noChanges)).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
