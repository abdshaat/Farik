import { type ReactNode, useEffect, useId, useRef } from "react";
import styles from "./Dialog.module.css";
import { uiStrings } from "./strings.ts";

export function Dialog({
	open,
	title,
	onClose,
	children,
	actions,
	fillsPhone,
	wide,
}: {
	open: boolean;
	title: string;
	onClose: () => void;
	children: ReactNode;
	actions?: ReactNode;
	/** At 480px and below, fill the screen and pin `actions` at the bottom. */
	fillsPhone?: boolean;
	/** A width of its own, 760 px where the screen has the room, whatever the dialog holds, so it does not grow as its states change. */
	wide?: boolean;
}) {
	const ref = useRef<HTMLDialogElement>(null);
	const titleId = useId();

	// Callers unmount the dialog rather than closing it, so the browser never gives focus back:
	// remember the opener (this effect runs before showModal's) and return to it on unmount.
	useEffect(() => {
		const opener = document.activeElement as HTMLElement | null;
		return () => {
			if (opener?.isConnected) opener.focus();
		};
	}, []);

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
			data-fills-phone={fillsPhone ? "" : undefined}
			data-wide={wide ? "" : undefined}
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
