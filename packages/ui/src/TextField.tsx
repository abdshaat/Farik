import type { ReactNode } from "react";
import { InfoTip } from "./InfoTip.tsx";
import { uiStrings } from "./strings.ts";
import styles from "./TextField.module.css";

export function TextField({
	id,
	label,
	value,
	onChange,
	hint,
	info,
	error,
	type = "text",
	required,
}: {
	id: string;
	label: string;
	value: string;
	onChange: (value: string) => void;
	hint?: string;
	info?: ReactNode;
	error?: string;
	type?: "text" | "password" | "url";
	required?: boolean;
}) {
	const describedBy =
		[info && `${id}-info`, hint && `${id}-hint`, error && `${id}-error`]
			.filter(Boolean)
			.join(" ") || undefined;
	return (
		<div className={styles.field}>
			{info ? (
				<div className={styles.head}>
					<label htmlFor={id} className={styles.label}>
						{label}
						{required ? ` (${uiStrings.required})` : null}
					</label>
					<InfoTip id={`${id}-info`}>{info}</InfoTip>
				</div>
			) : (
				<label htmlFor={id} className={styles.label}>
					{label}
					{required ? ` (${uiStrings.required})` : null}
				</label>
			)}
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
