import styles from "../pages.module.css";

/**
 * A day the owner picks or types, as `2026-10-08`; empty when they clear it. `min` is the first
 * day the picker offers, and `error` says why the day typed is not taken.
 */
export function DayField({
	id,
	label,
	value,
	onChange,
	hint,
	min,
	error,
}: {
	id: string;
	label: string;
	value: string;
	onChange: (value: string) => void;
	hint?: string | undefined;
	min?: string | undefined;
	error?: string | undefined;
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
				min={min}
				aria-invalid={error ? true : undefined}
				aria-describedby={
					[hint && `${id}-hint`, error && `${id}-error`]
						.filter(Boolean)
						.join(" ") || undefined
				}
				onChange={(e) => onChange(e.target.value)}
			/>
			{error && (
				<p id={`${id}-error`} className={styles.dayError}>
					{error}
				</p>
			)}
		</div>
	);
}
