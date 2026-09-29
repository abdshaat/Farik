import styles from "./Switch.module.css";

export function Switch({
	id,
	label,
	checked,
	onChange,
	description,
}: {
	id: string;
	label: string;
	checked: boolean;
	onChange: (checked: boolean) => void;
	description?: string;
}) {
	return (
		<div className={styles.row}>
			<button
				type="button"
				id={id}
				role="switch"
				aria-checked={checked}
				aria-labelledby={`${id}-label`}
				aria-describedby={description ? `${id}-description` : undefined}
				className={`${styles.track} ${checked ? styles.on : ""}`}
				onClick={() => onChange(!checked)}
			>
				<span className={styles.thumb} />
			</button>
			<span className={styles.text}>
				<span id={`${id}-label`} className={styles.label}>
					{label}
				</span>
				{description ? (
					<span id={`${id}-description`} className={styles.description}>
						{description}
					</span>
				) : null}
			</span>
		</div>
	);
}
