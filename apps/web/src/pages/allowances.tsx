import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import styles from "./Allowances.module.css";
import type { KitService } from "./Team.tsx";

/** The most calls an allowance may be (ADR 0037); the daemon refuses more. */
export const MAX_ALLOWANCE = 1000;

/** One allowance with the calls made of its tool this period, as `allowances.list` gives it. */
export type AllowanceRow = {
	agent: string;
	server: string;
	tool: string;
	/** The kit's plural noun for what a call makes. */
	what: string;
	used: number;
	of: number;
};
export type Allowances = {
	period: { kind: "sprint"; sprintId: string } | { kind: "day"; day: string };
	rows: AllowanceRow[];
};

/** Each allowance of every connected kit connector, and the period it is counted in. */
export function useAllowances() {
	return useQuery<Allowances>("allowances.list", {}).data;
}

/** `items` read as a list: "a", "a and b", "a, b and c". */
export const listed = (items: string[]) =>
	items.length < 3
		? items.join(" and ")
		: `${items.slice(0, -1).join(", ")} and ${items.at(-1)}`;

/** "14 of 20 images". */
export const countSaid = (row: AllowanceRow) =>
	t("allowOf", { used: row.used, of: row.of, what: row.what });

/**
 * The board's line for one allowance: how many were made against how many were allowed, and at the
 * number, or past it, that the agent asks first. Past it, the extras were ones the user approved,
 * since each call beyond the number waited for a yes.
 */
export function boardLine(
	row: AllowanceRow,
	name: string,
	period: Allowances["period"],
) {
	const fill = { name, used: row.used, of: row.of, what: row.what };
	if (row.used < row.of)
		return t(
			period.kind === "day" ? "allowBoardDay" : "allowBoardSprint",
			fill,
		);
	return row.used > row.of
		? `${t("allowBoardAsks", fill)} ${t("allowBoardExtra", fill)}`
		: t("allowBoardAsks", fill);
}

const capital = (words: string) =>
	words.charAt(0).toUpperCase() + words.slice(1);

/** The number a field holds, or undefined when it is not a whole number from 0 to 1,000. */
export function numberOf(text: string): number | undefined {
	const n = Number(text);
	return text.trim() !== "" &&
		Number.isInteger(n) &&
		n >= 0 &&
		n <= MAX_ALLOWANCE
		? n
		: undefined;
}

/** The words under a field that holds something no allowance may be, or none when it is fine. */
export function fieldError(text: string): string | undefined {
	if (numberOf(text) !== undefined) return undefined;
	return Number(text) > MAX_ALLOWANCE && Number.isFinite(Number(text))
		? t("allowTooMany")
		: t("allowWhole");
}

/** What the kit offers to allow, with each tool in the user's words. */
export type Offer = { tool: string; what: string; label: string };

export const offersOf = (service: KitService): Offer[] =>
	(service.allowances ?? []).map((offer) => ({
		tool: offer.tool,
		what: offer.what,
		label: service.labels[offer.tool] ?? offer.tool.replace(/[_-]+/g, " "),
	}));

/**
 * One number field for each tool the kit gives an allowance: what it makes, the tool in the user's
 * words, the number, "each sprint", and how many were made so far when that is known. A field that
 * holds more than 1,000 says so at itself.
 */
export function AllowanceFields({
	offers,
	values,
	made,
	shown,
	onChange,
}: {
	offers: Offer[];
	values: Record<string, string>;
	/** Calls made so far this period, by tool, when known. */
	made?: Record<string, number>;
	/** Whether to say what is wrong with a field. */
	shown: boolean;
	onChange: (tool: string, text: string) => void;
}) {
	return (
		<ul className={styles.fields}>
			{offers.map((offer, i) => {
				const text = values[offer.tool] ?? "";
				const error = shown ? fieldError(text) : undefined;
				const id = `allowance-${i}`;
				return (
					<li key={offer.tool} className={styles.field}>
						<label htmlFor={id} className={styles.label}>
							{capital(offer.what)}
						</label>
						<p className={styles.what} id={`${id}-what`}>
							{offer.label}
						</p>
						<span className={styles.line}>
							<input
								id={id}
								type="number"
								inputMode="numeric"
								min={0}
								max={MAX_ALLOWANCE}
								className={`${styles.input} ${error ? styles.invalid : ""}`}
								value={text}
								aria-invalid={error ? true : undefined}
								aria-describedby={[`${id}-what`, error && `${id}-error`]
									.filter(Boolean)
									.join(" ")}
								onChange={(e) => onChange(offer.tool, e.target.value)}
							/>
							<span className={styles.after}>{t("allowEach")}</span>
						</span>
						{made?.[offer.tool] !== undefined && (
							<p className={styles.after}>
								{t("allowMade", { n: made[offer.tool] ?? 0 })}
							</p>
						)}
						{error && (
							<p id={`${id}-error`} className={styles.error}>
								{error}
							</p>
						)}
					</li>
				);
			})}
		</ul>
	);
}
