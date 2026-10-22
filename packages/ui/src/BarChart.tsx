import styles from "./BarChart.module.css";
import { Table } from "./Table.tsx";

/** One kind of amount a bar is made of, such as "You, in Claude Code". */
export type BarSeries = { key: string; label: string };

/** One bar: its name, one value per series, and its numbers written as they are shown beside it. */
export type BarRow = {
	key: string;
	/** The bar's name, as the table and a screen reader say it. */
	label: string;
	/** What is written beside the bar in place of `label`, when `label` is too long for the space. */
	short?: string;
	values: number[];
	text: string;
};

/** The space a bar's drawing is laid out in, along the bar and across it. */
const LENGTH = 100;
const THICK = 8;

/**
 * Bars drawn as plain SVG in the app's tokens, with every bar's numbers written in text beside it
 * and the same numbers in a table behind a disclosure. `across` lays a bar per row along a line
 * (stacked when there are several series); `up` stands the bars as columns with the number above.
 */
export function BarChart({
	caption,
	series,
	rows,
	direction,
	format,
	tableSummary,
	labelColumn,
}: {
	caption: string;
	series: BarSeries[];
	rows: BarRow[];
	direction: "across" | "up";
	format: (value: number) => string;
	tableSummary: string;
	/** The heading of the table's first column. */
	labelColumn: string;
}) {
	const total = (row: BarRow) =>
		row.values.reduce((sum, value) => sum + value, 0);
	const largest = Math.max(0, ...rows.map(total));
	const share = (value: number) =>
		largest > 0 ? (value / largest) * LENGTH : 0;
	const said = (row: BarRow) =>
		`${row.label}: ${series
			.map((one, at) => `${one.label} ${format(row.values[at] ?? 0)}`)
			.join(", ")}`;
	const up = direction === "up";
	return (
		<figure className={styles.chart}>
			{series.length > 1 && (
				<ul className={styles.legend}>
					{series.map((one, at) => (
						<li key={one.key}>
							<i className={`${styles.swatch} ${styles[`tone${at % 2}`]}`} />
							{one.label}
						</li>
					))}
				</ul>
			)}
			<ul className={up ? styles.up : styles.across}>
				{rows.map((row) => {
					let before = 0;
					const bars = row.values.map((value, at) => {
						const length = share(value);
						const rect = up ? (
							<rect
								key={series[at]?.key ?? at}
								className={styles[`tone${at % 2}`]}
								x="0"
								y={LENGTH - before - length}
								width={THICK}
								height={length}
							/>
						) : (
							<rect
								key={series[at]?.key ?? at}
								className={styles[`tone${at % 2}`]}
								x={before}
								y="0"
								width={length}
								height={THICK}
							/>
						);
						before += length;
						return rect;
					});
					const drawing = (
						<svg
							role="img"
							aria-label={said(row)}
							className={up ? styles.column : styles.line}
							viewBox={up ? `0 0 ${THICK} ${LENGTH}` : `0 0 ${LENGTH} ${THICK}`}
							preserveAspectRatio="none"
						>
							{bars}
						</svg>
					);
					return (
						<li key={row.key} className={styles.item}>
							{up ? (
								<>
									<span className={styles.number}>{row.text}</span>
									{drawing}
									<span className={styles.name}>{row.short ?? row.label}</span>
								</>
							) : (
								<>
									<span className={styles.name}>{row.short ?? row.label}</span>
									{drawing}
									<span className={styles.number}>{row.text}</span>
								</>
							)}
						</li>
					);
				})}
			</ul>
			<details className={styles.more}>
				<summary>{tableSummary}</summary>
				<Table
					caption={caption}
					rows={rows}
					getKey={(row) => row.key}
					columns={[
						{ key: "label", header: labelColumn, render: (row) => row.label },
						...series.map((one, at) => ({
							key: one.key,
							header: one.label,
							align: "end" as const,
							render: (row: BarRow) => format(row.values[at] ?? 0),
						})),
					]}
				/>
			</details>
		</figure>
	);
}
