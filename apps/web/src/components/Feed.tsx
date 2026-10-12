import { Button } from "@catervas/ui";
import {
	type ReactElement,
	type ReactNode,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { t } from "../strings/t.ts";
import styles from "./Feed.module.css";

/** How near an edge of the box counts as being at it, in pixels: one row's room. */
const EDGE = 48;

/**
 * The scrolling part of a chat: its messages between the chat's head and its box. It opens at the
 * newest message, follows new ones while the user is at the bottom, keeps the user's place when an
 * earlier page arrives, and asks for the page before on reaching the top.
 */
export function Feed({
	labelledBy,
	newest,
	oldest,
	more,
	paging,
	onEarlier,
	notes,
	children,
}: {
	/** The id of the chat's `h1`, which names the box. */
	labelledBy: string;
	/** The newest message; `mine` when the user wrote it. */
	newest: { seq: number; mine: boolean } | undefined;
	/** The oldest message's seq. */
	oldest: number | undefined;
	/** Whether an earlier page exists. */
	more: boolean;
	paging: "busy" | "failed" | undefined;
	onEarlier: () => void;
	/** Above the messages, inside the box. */
	notes?: ReactNode;
	/** The page's list of messages. */
	children: ReactNode;
}): ReactElement {
	const box = useRef<HTMLElement>(null);
	// Where the previous run left things, to tell what changed in this one.
	const last = useRef<{
		newest?: number | undefined;
		oldest?: number | undefined;
		top?: number | undefined;
		height: number;
	}>({ height: 0 });
	const atBottom = useRef(true);
	const [behind, setBehind] = useState(false);

	const measure = (el: HTMLElement) => {
		atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight <= EDGE;
	};
	const toNewest = (el: HTMLElement) => {
		el.scrollTop = el.scrollHeight;
		measure(el);
	};

	// After every render: the box, a row or the page may all have changed size, and every size
	// change here comes with a render of the page.
	useLayoutEffect(() => {
		const el = box.current;
		if (!el) return;
		const before = last.current;
		// The page moved the view itself, as a meeting link's scrollIntoView does, before any
		// scroll event came to say so: where it left the view is where the user is.
		if (before.top !== undefined && el.scrollTop !== before.top)
			atBottom.current =
				Math.min(before.height, el.scrollHeight) -
					el.scrollTop -
					el.clientHeight <=
				EDGE;
		if (before.oldest !== undefined && oldest !== before.oldest) {
			el.scrollTop += el.scrollHeight - before.height;
			measure(el);
		} else if (newest && newest.seq !== before.newest) {
			if (newest.mine || atBottom.current) toNewest(el);
			else setBehind(true);
		} else if (atBottom.current) toNewest(el);
		last.current = {
			newest: newest?.seq,
			oldest,
			height: el.scrollHeight,
			top: el.scrollTop,
		};
	});

	const scrolled = () => {
		const el = box.current;
		if (!el) return;
		measure(el);
		if (atBottom.current) setBehind(false);
		if (el.scrollTop <= EDGE && more && paging === undefined) onEarlier();
	};
	const jump = () => {
		const el = box.current;
		if (!el) return;
		toNewest(el);
		setBehind(false);
		el.focus();
	};

	return (
		<div className={styles.feed}>
			<section
				ref={box}
				className={styles.scroller}
				aria-labelledby={labelledBy}
				// biome-ignore lint/a11y/noNoninteractiveTabindex: a scroll box must take focus to scroll by keyboard
				tabIndex={0}
				onScroll={scrolled}
			>
				{more && (
					<div>
						<Button onClick={onEarlier} busy={paging === "busy"}>
							{t("channelEarlier")}
						</Button>
					</div>
				)}
				{paging === "failed" && (
					<p role="alert" className={styles.alert}>
						{t("channelEarlierFailed")}
					</p>
				)}
				{notes}
				{children}
			</section>
			{behind && (
				<div className={styles.jump}>
					<Button kind="primary" onClick={jump}>
						{t("chatsNewMessages")}{" "}
						<svg
							viewBox="0 0 24 24"
							width="16"
							height="16"
							fill="none"
							aria-hidden="true"
						>
							<path
								d="M12 5v14M5 12l7 7 7-7"
								stroke="currentColor"
								strokeWidth="2"
							/>
						</svg>
					</Button>
				</div>
			)}
		</div>
	);
}
