import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Bullets } from "./Bullets.tsx";

describe("Bullets", () => {
	it("shows each line as a bullet", () => {
		for (const text of ["One.\nTwo.", "One.\n\nTwo.\n"]) {
			const { unmount } = render(<Bullets text={text} />);
			const items = within(screen.getByRole("list")).getAllByRole("listitem");
			expect(items.map((i) => i.textContent)).toEqual(["One.", "Two."]);
			unmount();
		}
	});
});
