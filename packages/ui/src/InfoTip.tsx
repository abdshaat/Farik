import { type ReactNode, useLayoutEffect, useRef, useState } from "react";
import styles from "./InfoTip.module.css";
import { uiStrings } from "./strings.ts";

const EDGE = 16;

/**
 * The tip stays in the DOM. It shows while the pointer is over it, while the
 * button is open (a click, or keyboard focus), and not after Escape or a second
 * click until the pointer leaves.
 */
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
	const [shut, setShut] = useState(false);
	const tip = useRef<HTMLSpanElement>(null);
	const pointer = useRef(false);

	/** Slide the tip sideways so it stays on the screen. */
	function fit() {
		const el = tip.current;
		if (!el) return;
		el.style.translate = "";
		const { left, right } = el.getBoundingClientRect();
		let shift = 0;
		if (right > window.innerWidth - EDGE)
			shift = window.innerWidth - EDGE - right;
		if (left + shift < EDGE) shift = EDGE - left;
		if (shift !== 0) el.style.translate = `${shift}px`;
	}
	function unfit() {
		if (tip.current) tip.current.style.translate = "";
	}
	// A closed tip is display: none and cannot be measured; fit once it shows.
	// biome-ignore lint/correctness/useExhaustiveDependencies: fit and unfit only read refs; the effect must run on open alone
	useLayoutEffect(() => {
		if (open) fit();
		else unfit();
	}, [open]);
	function show() {
		setOpen(true);
		setShut(false);
	}
	function close() {
		setOpen(false);
		setShut(true);
	}

	return (
		// biome-ignore lint/a11y/noStaticElementInteractions: hover only repositions and un-hushes the tip; the button carries the keyboard and screen-reader behaviour
		<span
			className={styles.wrap}
			onMouseEnter={fit}
			onMouseLeave={() => {
				setShut(false);
				if (!open) unfit();
			}}
		>
			<button
				onKeyDown={(e) => {
					if (e.key === "Escape") close();
				}}
				onMouseDown={() => {
					pointer.current = true;
				}}
				onFocus={() => {
					// A click focuses the button too; only keyboard focus opens the tip.
					if (!pointer.current) show();
				}}
				onBlur={() => {
					pointer.current = false;
					setOpen(false);
				}}
				type="button"
				className={styles.button}
				aria-label={label}
				aria-describedby={id}
				aria-expanded={open}
				onClick={() => (open ? close() : show())}
			>
				<svg
					aria-hidden="true"
					width="16"
					height="16"
					viewBox="0 0 16 16"
					fill="none"
					stroke="currentColor"
					strokeWidth="1.5"
				>
					<circle cx="8" cy="8" r="6.5" />
					<path d="M8 7v4" strokeLinecap="round" />
					<circle cx="8" cy="4.75" r="0.5" fill="currentColor" />
				</svg>
			</button>
			<span
				id={id}
				ref={tip}
				role="tooltip"
				className={styles.tip}
				data-open={open ? "" : undefined}
				data-shut={shut ? "" : undefined}
			>
				{children}
			</span>
		</span>
	);
}
