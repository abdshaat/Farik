import styles from "./StatusWord.module.css";

/** A status in words; `pill` sets a done word on the pale green pill (the computer check's "Ready"). */
export function StatusWord({
	tone,
	pill = false,
	children,
}: {
	tone: "done" | "working" | "waiting";
	pill?: boolean;
	children: string;
}) {
	const pale = pill ? ` ${styles.pill}` : "";
	return (
		<span className={`${styles.word} ${styles[tone]}${pale}`}>{children}</span>
	);
}
