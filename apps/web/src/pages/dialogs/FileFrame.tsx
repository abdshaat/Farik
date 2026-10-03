import styles from "../pages.module.css";
import { visibly } from "./ToolApproval.tsx";

/** One file of a skill from the project: its path above it, its text written out in an `untrusted` frame. */
export function FileFrame({ path, text }: { path: string; text: string }) {
	return (
		<>
			<p className={styles.toolHint}>
				<code>{path}</code>
			</p>
			<section
				aria-label={path}
				data-trust="untrusted"
				className={styles.untrusted}
				// biome-ignore lint/a11y/noNoninteractiveTabindex: a scroll box must take focus to scroll by keyboard
				tabIndex={0}
			>
				<pre>{visibly(text)}</pre>
			</section>
		</>
	);
}

/** The files of a skill, `SKILL.md` first and the rest by path. */
export const inOrder = (files: Record<string, string>) =>
	Object.entries(files).sort(([a], [b]) =>
		a === "SKILL.md" ? -1 : b === "SKILL.md" ? 1 : a.localeCompare(b),
	);
