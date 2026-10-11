import styles from "./Bullets.module.css";

/** An explanation as bullet points: one item per line of `text`. */
export function Bullets({ text }: { text: string }) {
	return (
		<ul className={styles.bullets}>
			{text
				.split("\n")
				.filter((line) => line.trim() !== "")
				.map((line) => (
					<li key={line}>{line}</li>
				))}
		</ul>
	);
}
