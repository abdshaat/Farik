import type { ReactNode } from "react";
import { InfoTip } from "./InfoTip.tsx";
import styles from "./Switch.module.css";

export function Switch({
	id,
	label,
	checked,
	onChange,
	description,
	info,
	icon,
}: {
	id: string;
	label: string;
	checked: boolean;
	onChange: (checked: boolean) => void;
	description?: string;
	/** A longer note, kept behind an info button after the label. */
	info?: ReactNode;
	/** Sits inside the label, before its words. */
	icon?: ReactNode;
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
				<span className={styles.labelRow}>
					<span id={`${id}-label`} className={styles.label}>
						{icon}
						{label}
					</span>
					{info ? <InfoTip id={`${id}-info`}>{info}</InfoTip> : null}
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
