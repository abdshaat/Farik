import styles from "./Stepper.module.css";
import { uiStrings } from "./strings.ts";

export function Stepper({
	steps,
	current,
	onPick,
}: {
	steps: string[];
	current: number;
	/** Given, each step before the current one is a link back to it. */
	onPick?: (index: number) => void;
}) {
	return (
		<ol className={styles.steps}>
			{steps.map((label, i) => (
				<li
					key={label}
					className={i === current ? styles.current : styles.step}
					aria-current={i === current ? "step" : undefined}
				>
					<span className={styles.number} aria-hidden="true">
						{i + 1}
					</span>{" "}
					{onPick && i < current ? (
						<button
							type="button"
							className={styles.link}
							onClick={() => onPick(i)}
						>
							{label}
						</button>
					) : (
						label
					)}
					{i === current ? (
						<span className={styles.hidden}>
							{" "}
							{uiStrings.stepOf(current + 1, steps.length)}
						</span>
					) : i < current ? (
						<span className={styles.hidden}>{uiStrings.stepDone}</span>
					) : null}
				</li>
			))}
		</ol>
	);
}
