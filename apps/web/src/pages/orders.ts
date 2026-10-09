// The Procurement Specialist's purchase orders and renewals as the daemon gives them
// (`purchase_orders.list`, `renewals.list`, and the `purchase_order` row of `waiting.list`), and the
// words and days Today and the agent's page say them with (spec 6.10, ADR 0039).
// Days are `YYYY-MM-DD` days or times; a time is said in the browser's own time zone, as the
// sites on the same page say theirs.

import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";
import type { OrderSend } from "./sellerMail.ts";

const DAY_MS = 86_400_000;

/** One line of an order. Every text in it is the agent's own. */
export type OrderLine = {
	item: string;
	quantity: number;
	unit: string;
	unitPrice: string;
	lineTotal: string;
};

/** A purchase order that waits for the owner, as `waiting.list` gives it. */
export type OrderAsk = {
	order: number;
	seller: string;
	sellerContact: string;
	lines: OrderLine[];
	currency: string;
	period: string;
	total: string;
	delivery: string;
	terms: string;
	/** The seller's page exactly as the agent wrote it, or empty. */
	url: string;
	/** The site that address names, in its ASCII form; absent with no address. */
	host?: string;
	evaluation: string;
	why: string;
	at: string;
	expiresAt: string;
	/** The order's email to the seller, when a mailbox can send it (6.10, ADR 0039). */
	send?: OrderSend;
};

/** What the latest follow-up learned, or what the owner corrected it to. */
export type OrderStatus = {
	status: string;
	note: string;
	expectedOn?: string;
	by: "agent" | "owner";
	at: string;
};

/** One order as `purchase_orders.list` gives it: where it stands and every day the log holds. */
export type OrderItem = {
	order: number;
	state: string;
	seller: string;
	total: string;
	currency: string;
	period: string;
	taskId: string;
	agentId: string;
	draftedAt: string;
	decidedAt?: string;
	note?: string;
	closeNote?: string;
	placedOn?: string;
	paid?: string;
	paidCurrency?: string;
	receivedOn?: string;
	renewsOn?: string;
	endedAt?: string;
	status?: OrderStatus;
	expiresAt?: string;
	overdue: boolean;
};

/** A renewal coming up, as `renewals.list` gives it. The vendor is the register's own text. */
export type Renewal = {
	renewal: number;
	vendor: string;
	renewsOn: string;
	decideBy: string;
	flaggedAt: string;
};

/** Every follow-up status, in the order the owner chooses among them, with its words. */
export const STATUSES = [
	["preparing", "orderStatusPreparing"],
	["shipped", "orderStatusShipped"],
	["delayed", "orderStatusDelayed"],
	["problem", "orderStatusProblem"],
] as const satisfies readonly (readonly [string, keyof typeof en])[];

/** A status in words; one this page does not know is shown as sent. */
export function statusWords(status: string): string {
	const known = STATUSES.find(([name]) => name === status);
	return known ? t(known[1]) : status;
}

const PERIODS: Record<string, keyof typeof en> = {
	once: "orderPeriodOnce",
	month: "orderPeriodMonth",
	year: "orderPeriodYear",
};

/** "once", "a month" or "a year". */
export const periodWords = (period: string) =>
	PERIODS[period] ? t(PERIODS[period]) : period;

const money = new Intl.NumberFormat("en-US", {
	minimumFractionDigits: 2,
	maximumFractionDigits: 2,
});

/** "1,450.00", from "1450.00". */
export const figure = (value: string) => money.format(Number(value));

/** "1,450.00 USD", from "1450.00" and "USD". */
export const amount = (value: string, currency: string) =>
	`${figure(value)} ${currency}`;

/** The day, counted in days from the epoch, of a `YYYY-MM-DD` day or of a time in this time zone. */
function dayNumber(when: string): number {
	const iso = /^(\d{4})-(\d{2})-(\d{2})$/.exec(when);
	if (iso)
		return (
			Date.UTC(Number(iso[1]), Number(iso[2]) - 1, Number(iso[3])) / DAY_MS
		);
	const date = new Date(when);
	return Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / DAY_MS;
}

const today = (now: Date) =>
	Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()) / DAY_MS;

/** "6 November", or "6 November 2027" when the day is not in this year. */
export function calendarDay(when: string, now: Date): string {
	const date = new Date(dayNumber(when) * DAY_MS);
	return new Intl.DateTimeFormat("en-GB", {
		day: "numeric",
		month: "long",
		...(date.getUTCFullYear() === now.getFullYear() ? {} : { year: "numeric" }),
		timeZone: "UTC",
	}).format(date);
}

/** A day that has passed: "today", "yesterday" or "on 2 October". */
export function pastDay(when: string, now: Date): string {
	const ago = today(now) - dayNumber(when);
	if (ago === 0) return t("siteDayToday");
	if (ago === 1) return t("siteDayYesterday");
	return t("siteDayOn", { day: calendarDay(when, now) });
}

/** A day to come: "today", "tomorrow" or "6 November"; one that has passed is said as a date. */
export function aheadDay(when: string, now: Date): string {
	const ahead = dayNumber(when) - today(now);
	if (ahead === 0) return t("siteDayToday");
	if (ahead === 1) return t("orderDayTomorrow");
	return calendarDay(when, now);
}

/** Today's `YYYY-MM-DD` in this time zone. */
export function todayIso(now: Date): string {
	return new Date(today(now) * DAY_MS).toISOString().slice(0, 10);
}

/** The day `days` after a `YYYY-MM-DD` day. */
export function afterDays(day: string, days: number): string {
	return new Date((dayNumber(day) + days) * DAY_MS).toISOString().slice(0, 10);
}

/** The most days a placed order is waited for when no follow-up gave a day (spec 6.10). */
export const WAITED_DAYS = 30;
