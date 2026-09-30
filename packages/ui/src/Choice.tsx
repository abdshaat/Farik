import styles from "./Choice.module.css";

export function Choice<V extends string>({
	name,
	legend,
	options,
	value,
	onChange,
	error,
}: {
	name: string;
	legend: string;
	options: { value: V; label: string; description?: string }[];
	value: V;
	onChange: (value: V) => void;
	/** Why the choice is refused, said under it and tied to the group. */
	error?: string;
}) {
	return (
		<fieldset
			className={styles.group}
			aria-describedby={error ? `${name}-error` : undefined}
		>
			<legend className={styles.legend}>{legend}</legend>
			{options.map((o) => (
				<label key={o.value} className={styles.card}>
					<input
						type="radio"
						className={styles.radio}
						name={name}
						value={o.value}
						checked={o.value === value}
						onChange={() => onChange(o.value)}
					/>
					<span className={styles.text}>
						<span className={styles.label}>{o.label}</span>
						{o.description ? (
							<span className={styles.description}>{o.description}</span>
						) : null}
					</span>
				</label>
			))}
			{error ? (
				<p id={`${name}-error`} className={styles.error}>
					{error}
				</p>
			) : null}
		</fieldset>
	);
}
