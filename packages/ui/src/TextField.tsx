import { uiStrings } from "./strings.ts";
import styles from "./TextField.module.css";

export function TextField({
	id,
	label,
	value,
	onChange,
	hint,
	error,
	type = "text",
	required,
}: {
	id: string;
	label: string;
	value: string;
	onChange: (value: string) => void;
	hint?: string;
	error?: string;
	type?: "text" | "password" | "url";
	required?: boolean;
}) {
	const describedBy =
		[hint && `${id}-hint`, error && `${id}-error`].filter(Boolean).join(" ") ||
		undefined;
	return (
		<div className={styles.field}>
			<label htmlFor={id} className={styles.label}>
				{label}
				{required ? ` (${uiStrings.required})` : null}
			</label>
			{hint ? (
				<p id={`${id}-hint`} className={styles.hint}>
					{hint}
				</p>
			) : null}
			<input
				id={id}
				type={type}
				className={`${styles.input} ${error ? styles.invalid : ""}`}
				value={value}
				required={required}
				aria-invalid={error ? true : undefined}
				aria-describedby={describedBy}
				onChange={(e) => onChange(e.target.value)}
			/>
			{error ? (
				<p id={`${id}-error`} className={styles.error}>
					{error}
				</p>
			) : null}
		</div>
	);
}
