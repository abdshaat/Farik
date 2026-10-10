import type { ReactNode } from "react";
import styles from "./Button.module.css";
import { uiStrings } from "./strings.ts";

export function Button({
	kind = "secondary",
	type = "button",
	disabled = false,
	busy = false,
	onClick,
	"aria-describedby": describedBy,
	children,
}: {
	kind?: "primary" | "secondary" | "quiet";
	type?: "button" | "submit";
	disabled?: boolean;
	busy?: boolean;
	onClick?: () => void;
	"aria-describedby"?: string;
	children: ReactNode;
}) {
	return (
		<button
			type={type}
			className={`${styles.button} ${styles[kind]}`}
			data-kind={kind}
			disabled={disabled || busy}
			aria-busy={busy || undefined}
			aria-describedby={describedBy}
			onClick={onClick}
		>
			{children}
			{busy ? (
				<>
					{" "}
					<span className={styles.hidden}>{uiStrings.busy}</span>
				</>
			) : null}
		</button>
	);
}
