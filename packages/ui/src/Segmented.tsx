import type { ReactNode } from "react";
import { InfoTip } from "./InfoTip.tsx";
import styles from "./Segmented.module.css";

/**
 * A few short choices side by side in one track, the chosen one filled: a radio group, so a
 * screen reader names it and the arrow keys move along it. On a narrow screen the segments wrap.
 */
export function Segmented<V extends string>({
	name,
	legend,
	options,
	value,
	onChange,
	info,
	hint,
}: {
	name: string;
	legend: string;
	options: { value: V; label: string }[];
	value: V | undefined;
	onChange: (value: V) => void;
	/** A note on the whole group, kept behind an info button by the legend. */
	info?: ReactNode;
	/** One short line under the track, such as what the chosen option means. */
	hint?: string;
}) {
	return (
		<fieldset
			role="radiogroup"
			className={styles.group}
			aria-describedby={hint ? `${name}-hint` : undefined}
		>
			<legend className={styles.legend}>{legend}</legend>
			{info ? (
				<InfoTip
					id={`${name}-info`}
					label={`More about ${legend.toLowerCase()}`}
				>
					{info}
				</InfoTip>
			) : null}
			<div className={styles.track}>
				{options.map((o) => (
					<label key={o.value} className={styles.segment}>
						<input
							type="radio"
							className={styles.radio}
							name={name}
							value={o.value}
							checked={o.value === value}
							onChange={() => onChange(o.value)}
						/>
						<span>{o.label}</span>
					</label>
				))}
			</div>
			{hint ? (
				<p id={`${name}-hint`} className={styles.hint}>
					{hint}
				</p>
			) : null}
		</fieldset>
	);
}
