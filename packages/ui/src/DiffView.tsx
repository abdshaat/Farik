import styles from "./DiffView.module.css";
import { parseDiff } from "./parse-diff.ts";
import { uiStrings } from "./strings.ts";

export function DiffView({ diff, label }: { diff: string; label: string }) {
	// ponytail: renders the whole diff at once; add windowing if a real diff is slow.
	const files = parseDiff(diff);
	return (
		<section className={styles.view} aria-label={label}>
			{files.length === 0 ? (
				<p className={styles.empty}>{uiStrings.noChanges}</p>
			) : (
				files.map((file, n) => (
					// biome-ignore lint/suspicious/noArrayIndexKey: the list is static per render
					<section key={n} className={styles.file}>
						<h3 className={styles.path}>{file.path}</h3>
						<div className={styles.block}>
							{file.lines.map((line, i) => (
								<div
									// biome-ignore lint/suspicious/noArrayIndexKey: the list is static per render
									key={i}
									className={`${styles.row} ${styles[line.kind]}`}
								>
									{line.kind === "added" ? (
										<>
											+{line.text}
											<span className={styles.hidden}>{uiStrings.added}</span>
										</>
									) : line.kind === "removed" ? (
										<>
											{"−"}
											{line.text}
											<span className={styles.hidden}>{uiStrings.removed}</span>
										</>
									) : (
										<>
											{line.kind === "context" ? " " : ""}
											{line.text}
										</>
									)}
								</div>
							))}
						</div>
					</section>
				))
			)}
		</section>
	);
}
