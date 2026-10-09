import { Fragment, type ReactNode } from "react";
import styles from "./Choice.module.css";
import { InfoTip } from "./InfoTip.tsx";

export function Choice<V extends string>({
	name,
	legend,
	options,
	value,
	onChange,
	error,
	info,
}: {
	name: string;
	legend: string;
	options: {
		value: V;
		label: string;
		description?: string;
		disabled?: boolean;
		/** Shown in the card after the description, such as a row of faces. */
		extra?: ReactNode;
		/** Shown after the card, inside the group, such as a further choice the option opens. */
		after?: ReactNode;
	}[];
	value: V;
	onChange: (value: V) => void;
	/** Why the choice is refused, said under it and tied to the group. */
	error?: string;
	/** A note on the whole group, kept behind an info button by the legend. */
	info?: ReactNode;
}) {
	return (
		<fieldset
			className={styles.group}
			aria-describedby={error ? `${name}-error` : undefined}
		>
			<legend className={styles.legend}>{legend}</legend>
			{info ? <InfoTip id={`${name}-info`}>{info}</InfoTip> : null}
			{options.map((o) => (
				<Fragment key={o.value}>
					<label className={styles.card}>
						<input
							type="radio"
							className={styles.radio}
							name={name}
							value={o.value}
							checked={o.value === value}
							disabled={o.disabled}
							onChange={() => onChange(o.value)}
						/>
						<span className={styles.text}>
							<span className={styles.label}>{o.label}</span>
							{o.description ? (
								<span className={styles.description}>{o.description}</span>
							) : null}
							{o.extra}
						</span>
					</label>
					{o.after}
				</Fragment>
			))}
			{error ? (
				<p id={`${name}-error`} className={styles.error}>
					{error}
				</p>
			) : null}
		</fieldset>
	);
}
