import styles from "./Stepper.module.css";
import { uiStrings } from "./strings.ts";

export function Stepper({
	steps,
	current,
}: {
	steps: string[];
	current: number;
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
					{label}
					{i === current ? (
						<span className={styles.hidden}>
							{" "}
							{uiStrings.stepOf(current + 1, steps.length)}
						</span>
					) : null}
				</li>
			))}
		</ol>
	);
}
