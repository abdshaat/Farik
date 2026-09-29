import type { ReactNode } from "react";
import styles from "./KanbanColumn.module.css";

export function KanbanColumn({
	id,
	title,
	count,
	children,
}: {
	id: string;
	title: string;
	count: number;
	children: ReactNode;
}) {
	const titleId = `${id}-title`;
	return (
		<section className={styles.column} aria-labelledby={titleId}>
			<h3 className={styles.heading}>
				<span id={titleId}>{title}</span>{" "}
				<span className={styles.count}>{count}</span>
			</h3>
			{children}
		</section>
	);
}
