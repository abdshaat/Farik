import { type ReactNode, useState } from "react";
import styles from "./InfoTip.module.css";
import { uiStrings } from "./strings.ts";

/** The tip stays in the DOM and is shown by CSS on hover and focus, or by a click (touch). */
export function InfoTip({
	id,
	label = uiStrings.infoLabel,
	children,
}: {
	id: string;
	label?: string;
	children: ReactNode;
}) {
	const [open, setOpen] = useState(false);
	return (
		<span className={`${styles.wrap} ${open ? styles.open : ""}`}>
			<button
				onKeyDown={(e) => {
					if (e.key === "Escape") setOpen(false);
				}}
				type="button"
				className={styles.button}
				aria-label={label}
				aria-describedby={id}
				aria-expanded={open}
				onClick={() => setOpen(!open)}
			>
				<span aria-hidden="true">ⓘ</span>
			</button>
			<span id={id} role="tooltip" className={styles.tip}>
				{children}
			</span>
		</span>
	);
}
