import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Table } from "./Table.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

type Row = { id: string; name: string; cost: string };
const rows: Row[] = [
	{ id: "1", name: "Ada", cost: "$1.00" },
	{ id: "2", name: "Bo", cost: "$2.50" },
];

describe("Table", () => {
	it("captions the table and heads each column", async () => {
		const { container } = render(
			<Table
				caption="Spending"
				rows={rows}
				getKey={(r) => r.id}
				columns={[
					{ key: "name", header: "Who", render: (r) => r.name },
					{ key: "cost", header: "Cost", align: "end", render: (r) => r.cost },
				]}
			/>,
		);
		const table = screen.getByRole("table", { name: "Spending" });
		expect(
			screen.getAllByRole("columnheader").map((h) => h.textContent),
		).toEqual(["Who", "Cost"]);
		expect(screen.getAllByRole("row")).toHaveLength(3);
		const cell = screen.getByText("$1.00");
		expect(cell.className).toMatch(/numeric/);
		expect(screen.getByText("Ada").className).not.toMatch(/numeric/);
		expect(table).toBeTruthy();
		await expectNoAxeViolations(container);
	});
});
