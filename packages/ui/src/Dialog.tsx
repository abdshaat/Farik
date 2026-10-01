import { type ReactNode, useEffect, useId, useRef } from "react";
import styles from "./Dialog.module.css";
import { uiStrings } from "./strings.ts";

export function Dialog({
	open,
	title,
	onClose,
	children,
	actions,
}: {
	open: boolean;
	title: string;
	onClose: () => void;
	children: ReactNode;
	actions?: ReactNode;
}) {
	const ref = useRef<HTMLDialogElement>(null);
	const titleId = useId();

	useEffect(() => {
		const dialog = ref.current;
		if (!dialog) return;
		if (open && !dialog.open) dialog.showModal();
		if (!open && dialog.open) dialog.close();
	}, [open]);

	return (
		<dialog
			ref={ref}
			className={styles.dialog}
			aria-labelledby={titleId}
			// The browser can close a modal itself (a form, a close request);
			// tell the parent so its state does not drift.
			onClose={() => open && onClose()}
			onCancel={(e) => {
				// React state is the source of truth: the parent closes us.
				e.preventDefault();
				onClose();
			}}
		>
			<header className={styles.header}>
				<h2 id={titleId} className={styles.title}>
					{title}
				</h2>
				<button type="button" className={styles.close} onClick={onClose}>
					{uiStrings.close}
				</button>
			</header>
			<div className={styles.body}>{children}</div>
			{actions ? <footer className={styles.actions}>{actions}</footer> : null}
		</dialog>
	);
}
