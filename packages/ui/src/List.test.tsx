import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { List } from "./List.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

const render_ = (items: string[]) =>
	render(
		<List
			label="Tasks"
			items={items}
			getKey={(s) => s}
			render={(s) => <span>{s}</span>}
			empty={<p>Nothing here</p>}
		/>,
	);

describe("List", () => {
	it("shows each item or the empty state", async () => {
		const { container, unmount } = render_(["a", "b", "c"]);
		const items = within(
			screen.getByRole("list", { name: "Tasks" }),
		).getAllByRole("listitem");
		expect(items.map((i) => i.textContent)).toEqual(["a", "b", "c"]);
		await expectNoAxeViolations(container);
		unmount();
		const empty = render_([]);
		expect(screen.queryAllByRole("listitem")).toHaveLength(0);
		expect(screen.getByText("Nothing here")).toBeTruthy();
		await expectNoAxeViolations(empty.container);
	});
});
