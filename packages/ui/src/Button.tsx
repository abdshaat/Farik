import type { ReactNode } from "react";
import styles from "./Button.module.css";
import { uiStrings } from "./strings.ts";

export function Button({
	kind = "secondary",
	type = "button",
	disabled = false,
	busy = false,
	onClick,
	children,
}: {
	kind?: "primary" | "secondary" | "quiet";
	type?: "button" | "submit";
	disabled?: boolean;
	busy?: boolean;
	onClick?: () => void;
	children: ReactNode;
}) {
	return (
		<button
			type={type}
			className={`${styles.button} ${styles[kind]}`}
			disabled={disabled || busy}
			aria-busy={busy || undefined}
			onClick={onClick}
		>
			{children}
			{busy ? ` ${uiStrings.busy}` : null}
		</button>
	);
}
