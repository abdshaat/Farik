import type { ReactNode } from "react";
import styles from "./List.module.css";

export function List<T>({
	label,
	items,
	getKey,
	render,
	empty,
}: {
	label: string;
	items: T[];
	getKey: (item: T) => string;
	render: (item: T) => ReactNode;
	empty: ReactNode;
}) {
	if (items.length === 0) return <>{empty}</>;
	return (
		<ul className={styles.list} aria-label={label}>
			{items.map((item) => (
				<li key={getKey(item)} className={styles.row}>
					{render(item)}
				</li>
			))}
		</ul>
	);
}
