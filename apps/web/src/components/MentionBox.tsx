import { type KeyboardEvent, useLayoutEffect, useRef, useState } from "react";
import type { Agent } from "../pages/setup/TeamSetup.tsx";
import { t } from "../strings/t.ts";
import styles from "./MentionBox.module.css";

/**
 * The composer's text box: typing `@` opens a listbox of the team's agents, which the arrow keys
 * move through and Enter picks, inserting `@<id>`.
 */
export function MentionBox({
	id,
	value,
	onChange,
	agents,
	describedBy,
}: {
	id: string;
	value: string;
	onChange: (value: string) => void;
	agents: Agent[];
	describedBy?: string;
}) {
	const box = useRef<HTMLTextAreaElement>(null);
	const [caret, setCaret] = useState(0);
	const [active, setActive] = useState(0);
	const [closedAt, setClosedAt] = useState<string>();
	const moveTo = useRef<number>(undefined);
	useLayoutEffect(() => {
		if (moveTo.current === undefined) return;
		box.current?.setSelectionRange(moveTo.current, moveTo.current);
		moveTo.current = undefined;
	});

	const before = value.slice(0, caret);
	const partial = /(?:^|\s)@([a-z0-9-]*)$/.exec(before)?.[1];
	const options =
		partial === undefined || closedAt === value
			? []
			: agents.filter(
					(a) =>
						a.status !== "retired" &&
						(a.id.startsWith(partial) ||
							a.displayName.toLowerCase().startsWith(partial)),
				);
	const open = options.length > 0;
	const current = Math.min(active, options.length - 1);
	const optionId = (i: number) => `${id}-mention-${i}`;

	const pick = (agent: Agent) => {
		const start = before.length - (partial?.length ?? 0) - 1;
		const inserted = `@${agent.id} `;
		moveTo.current = start + inserted.length;
		setCaret(start + inserted.length);
		onChange(value.slice(0, start) + inserted + value.slice(caret));
	};
	const onKeyDown = (e: KeyboardEvent) => {
		if (!open) return;
		const n = options.length;
		if (e.key === "ArrowDown") setActive((current + 1) % n);
		else if (e.key === "ArrowUp") setActive((current - 1 + n) % n);
		else if (e.key === "Enter") pick(options[current] as Agent);
		else if (e.key === "Escape") setClosedAt(value);
		else return;
		e.preventDefault();
	};

	return (
		<div className={styles.box}>
			<textarea
				ref={box}
				id={id}
				rows={2}
				value={value}
				placeholder={t("channelPlaceholder")}
				aria-describedby={describedBy}
				aria-autocomplete="list"
				aria-controls={open ? `${id}-mentions` : undefined}
				aria-activedescendant={open ? optionId(current) : undefined}
				onChange={(e) => {
					setCaret(e.target.selectionStart ?? e.target.value.length);
					setActive(0);
					onChange(e.target.value);
				}}
				onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
				onKeyDown={onKeyDown}
			/>
			{/* A textarea cannot say that its list opened, so this line does. */}
			<p aria-live="polite" className={styles.hidden}>
				{open && t("channelMentionCount", { n: options.length })}
			</p>
			{open && (
				<div
					role="listbox"
					id={`${id}-mentions`}
					aria-label={t("channelMentionList")}
					className={styles.list}
				>
					{options.map((agent, i) => (
						<div
							key={agent.id}
							id={optionId(i)}
							role="option"
							tabIndex={-1}
							aria-selected={i === current}
							className={styles.option}
							// Before the box loses focus, so the pick lands in it.
							onMouseDown={(e) => {
								e.preventDefault();
								pick(agent);
							}}
						>
							{agent.displayName}
						</div>
					))}
				</div>
			)}
		</div>
	);
}
