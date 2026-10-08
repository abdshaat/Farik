import styles from "../pages.module.css";

/** A day the owner picks or types, as `2026-10-08`; empty when they clear it. */
export function DayField({
	id,
	label,
	value,
	onChange,
	hint,
}: {
	id: string;
	label: string;
	value: string;
	onChange: (value: string) => void;
	hint?: string | undefined;
}) {
	return (
		<div className={styles.dayField}>
			<label htmlFor={id}>{label}</label>
			{hint && (
				<p id={`${id}-hint`} className={styles.orderHint}>
					{hint}
				</p>
			)}
			<input
				id={id}
				type="date"
				value={value}
				aria-describedby={hint ? `${id}-hint` : undefined}
				onChange={(e) => onChange(e.target.value)}
			/>
		</div>
	);
}
