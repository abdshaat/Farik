import styles from "./StatusWord.module.css";

export function StatusWord({
	tone,
	children,
}: {
	tone: "done" | "working" | "waiting";
	children: string;
}) {
	return <span className={`${styles.word} ${styles[tone]}`}>{children}</span>;
}
