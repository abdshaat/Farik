import { t } from "../strings/t.ts";
import styles from "./DocumentFrame.module.css";

/**
 * An HTML document, shown only in a frame with an empty `sandbox` (no `allow-*` token) from the
 * daemon's own route, which adds its policy and paints it in the app's theme. Its text never enters
 * this page's DOM. A new `version` (the blob) reloads the frame.
 */
export function DocumentFrame({
	path,
	task,
	version,
	title,
	className,
}: {
	path: string;
	task?: string | undefined;
	version?: string | undefined;
	title: string;
	className?: string | undefined;
}) {
	const src = `/documents/raw?path=${encodeURIComponent(path)}${
		task ? `&task=${encodeURIComponent(task)}` : ""
	}${version ? `&v=${encodeURIComponent(version)}` : ""}${
		document.documentElement.dataset.theme === "dark" ? "&theme=dark" : ""
	}`;
	return (
		<div>
			<a className={styles.open} href={src} target="_blank" rel="noopener">
				{t("docOpenFull")}
			</a>
			<iframe
				className={className ? `${styles.frame} ${className}` : styles.frame}
				sandbox=""
				src={src}
				title={title}
			/>
		</div>
	);
}
