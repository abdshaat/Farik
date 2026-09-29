import type { ReactNode } from "react";
import styles from "./Table.module.css";

export function Table<T>({
	caption,
	columns,
	rows,
	getKey,
}: {
	caption: string;
	columns: {
		key: string;
		header: string;
		align?: "start" | "end";
		render: (row: T) => ReactNode;
	}[];
	rows: T[];
	getKey: (row: T) => string;
}) {
	const cls = (align?: "start" | "end") =>
		align === "end" ? styles.numeric : undefined;
	return (
		<div className={styles.wrap}>
			<table className={styles.table}>
				<caption className={styles.caption}>{caption}</caption>
				<thead>
					<tr>
						{columns.map((c) => (
							<th key={c.key} scope="col" className={cls(c.align)}>
								{c.header}
							</th>
						))}
					</tr>
				</thead>
				<tbody>
					{rows.map((row) => (
						<tr key={getKey(row)}>
							{columns.map((c) => (
								<td key={c.key} className={cls(c.align)}>
									{c.render(row)}
								</td>
							))}
						</tr>
					))}
				</tbody>
			</table>
		</div>
	);
}
