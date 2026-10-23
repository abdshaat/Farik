import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BarChart } from "./BarChart.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

const dollars = (n: number) => `$${n.toFixed(2)}`;
const SERIES = [
	{ key: "you", label: "You, in Claude Code" },
	{ key: "agents", label: "The team’s agents" },
];
const ROWS = [
	{
		key: "t",
		label: "Tidepool",
		values: [41.2, 18.6],
		text: "$41.20 + $18.60",
	},
	{ key: "p", label: "Price Watch", values: [6.1, 2.4], text: "$6.10 + $2.40" },
];

/** The widths of the rects drawn in the image named `name`. */
const widths = (name: string) =>
	Array.from(
		screen.getByRole("img", { name }).querySelectorAll("rect"),
		(rect) => Number(rect.getAttribute("width")),
	);

describe("BarChart", () => {
	it("draws_bars_with_their_numbers", async () => {
		const { container, unmount } = render(
			<BarChart
				caption="Usage per team"
				series={SERIES}
				rows={ROWS}
				direction="across"
				format={dollars}
				tableSummary="See as a table"
				labelColumn="Team"
			/>,
		);
		// Each bar carries its numbers in text, beside it, and names them to a screen reader.
		expect(screen.getByText("$41.20 + $18.60")).toBeTruthy();
		expect(screen.getByText("$6.10 + $2.40")).toBeTruthy();
		const big =
			"Tidepool: You, in Claude Code $41.20, The team’s agents $18.60";
		// The longest bar fills the track; the other is drawn to the same scale.
		const [you, agents] = widths(big);
		expect((you ?? 0) + (agents ?? 0)).toBeCloseTo(100);
		const small = widths(
			"Price Watch: You, in Claude Code $6.10, The team’s agents $2.40",
		);
		expect(small[0]).toBeCloseTo((6.1 / 59.8) * 100);
		// The legend names each series, as does the table's column.
		for (const one of SERIES)
			expect(screen.getAllByText(one.label)).toHaveLength(2);

		// The table alternative holds the same numbers.
		const more = screen.getByText("See as a table").closest("details");
		if (!more) throw new Error("no table alternative");
		more.open = true;
		const table = within(more).getByRole("table", { name: "Usage per team" });
		expect(
			within(table)
				.getAllByRole("columnheader")
				.map((h) => h.textContent),
		).toEqual(["Team", "You, in Claude Code", "The team’s agents"]);
		const rows = within(table).getAllByRole("row").slice(1);
		expect(
			rows.map((row) =>
				within(row)
					.getAllByRole("cell")
					.map((c) => c.textContent),
			),
		).toEqual([
			["Tidepool", "$41.20", "$18.60"],
			["Price Watch", "$6.10", "$2.40"],
		]);
		await expectNoAxeViolations(container);
		unmount();

		// Columns, with the number above each.
		const days = ["Monday", "Tuesday", "Wednesday"].map((label, at) => ({
			key: label,
			label,
			short: label.slice(0, 1),
			values: [[4, 6, 3][at] ?? 0],
			text: String([4, 6, 3][at]),
		}));
		render(
			<BarChart
				caption="Sessions per day"
				series={[{ key: "n", label: "Sessions" }]}
				rows={days}
				direction="up"
				format={String}
				tableSummary="See as a table"
				labelColumn="Day"
			/>,
		);
		// The tallest column is full height; the number is written above each.
		const tall = screen.getByRole("img", { name: "Wednesday: Sessions 3" });
		expect(tall).toBeTruthy();
		expect(
			Number(
				screen
					.getByRole("img", { name: "Tuesday: Sessions 6" })
					.querySelector("rect")
					?.getAttribute("height"),
			),
		).toBeCloseTo(100);
		expect(
			Number(tall.querySelector("rect")?.getAttribute("height")),
		).toBeCloseTo(50);
		// The name beside a column is the short one; the table says it in full.
		expect(screen.getAllByText("W")).toHaveLength(1);
		expect(screen.getAllByText("Wednesday")).toHaveLength(1);
		// Once above its column and once in the table alternative.
		expect(screen.getAllByText("6")).toHaveLength(2);
	});
});
