import { type ReactNode, useId } from "react";
import { t } from "../strings/t.ts";
import styles from "./Chats.module.css";
import { visibly } from "./dialogs/ToolApproval.tsx";

/** What the Product Manager heard in a new project's interview, as `chat.messages` has it. */
export type HeardSummary = {
	customers: string;
	problem: string;
	must: string;
	notNow: string;
	worksWhen: string;
};

const PARTS = [
	["customers", "heardCustomers"],
	["problem", "heardProblem"],
	["must", "heardMust"],
	["notNow", "heardNotNow"],
	["worksWhen", "heardWorksWhen"],
] as const;

/**
 * "What I heard": the five parts of a summary as text with their line breaks (the agent's words,
 * never markup), and, below them, what follows from it (`children`).
 */
export function Heard({
	heard,
	children,
}: {
	heard: HeardSummary;
	children?: ReactNode;
}) {
	const title = useId();
	return (
		<section className={styles.heard} aria-labelledby={title}>
			<h2 id={title} className={styles.heardTitle}>
				{t("heardTitle")}
			</h2>
			<dl className={styles.heardParts}>
				{PARTS.map(([part, label]) => (
					<div key={part} className={styles.heardPart}>
						<dt>{t(label)}</dt>
						<dd className={styles.lines}>{visibly(heard[part])}</dd>
					</div>
				))}
			</dl>
			{children}
		</section>
	);
}
