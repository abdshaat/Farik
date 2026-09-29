import styles from "./Choice.module.css";

export function Choice<V extends string>({
	name,
	legend,
	options,
	value,
	onChange,
}: {
	name: string;
	legend: string;
	options: { value: V; label: string; description?: string }[];
	value: V;
	onChange: (value: V) => void;
}) {
	return (
		<fieldset className={styles.group}>
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
		</fieldset>
	);
}
