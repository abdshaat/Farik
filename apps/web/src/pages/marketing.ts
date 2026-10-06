// The marketing plan's words and dates, shared by Today's row and the plan's page (ADR 0042).
// Dates are `YYYY-MM-DD` days or UTC times, said in UTC as the team's own lines are.

import type { en } from "../strings/en.ts";
import { t } from "../strings/t.ts";

const DAY_MS = 86_400_000;

/** The parts of `date` in UTC as `Intl` words them, by type. */
function parts(
	date: string,
	options: Intl.DateTimeFormatOptions,
): Record<string, string> {
	return Object.fromEntries(
		new Intl.DateTimeFormat("en-GB", { ...options, timeZone: "UTC" })
			.formatToParts(new Date(date))
			.map((part) => [part.type, part.value]),
	);
}

/** "Monday 12 October". */
export function longDay(date: string): string {
	const p = parts(date, { weekday: "long", day: "numeric", month: "long" });
	return `${p.weekday} ${p.day} ${p.month}`;
}

/** "Mon 12 Oct". */
export function shortDay(date: string): string {
	const p = parts(date, { weekday: "short", day: "numeric", month: "short" });
	return `${p.weekday} ${p.day} ${p.month}`;
}

/** "12 Oct". */
function dayMonth(date: string): string {
	const p = parts(date, { day: "numeric", month: "short" });
	return `${p.day} ${p.month}`;
}

/** "Tuesday 6 October at 08:40", from a time. */
export function when(time: string): string {
	return `${longDay(time)} at ${time.slice(11, 16)}`;
}

/** "12 Oct to 22 Nov". */
export function shortRange(from: string, to: string): string {
	return t("marketingDates", { from: dayMonth(from), to: dayMonth(to) });
}

/** "Monday 12 October to Sunday 22 November". */
export function longRange(from: string, to: string): string {
	return t("marketingDates", { from: longDay(from), to: longDay(to) });
}

/** "$450.00", from "450.00", in the plan's currency. */
export function money(amount: string, currency: string): string {
	return new Intl.NumberFormat("en-US", { style: "currency", currency }).format(
		Number(amount),
	);
}

/** Today in UTC, as a day. */
export const today = () => new Date().toISOString().slice(0, 10);

const days = (from: string, to: string) =>
	Math.round((Date.parse(to) - Date.parse(from)) / DAY_MS);

/** How many days a plan runs, its first and last included. */
export const length = (from: string, to: string) => days(from, to) + 1;

/** Which day of the plan `now` is, from 1, kept within the plan. */
export const dayOf = (from: string, to: string, now: string) =>
	Math.min(length(from, to), Math.max(1, days(from, now) + 1));

/** "6 weeks" for whole weeks, else "10 days". */
export function runs(from: string, to: string): string {
	const n = length(from, to);
	return n % 7 === 0
		? t("marketingWeeks", { n: n / 7 })
		: t("marketingDays", { n });
}

const CURRENCIES: Record<string, keyof typeof en> = {
	USD: "currencyUsd",
	EUR: "currencyEur",
	GBP: "currencyGbp",
	CAD: "currencyCad",
	AUD: "currencyAud",
};

/** "US dollars" for USD, and the code itself for a currency with no words yet. */
export const currencyWords = (code: string) =>
	CURRENCIES[code] ? t(CURRENCIES[code]) : code;

const CHANNELS: Record<string, keyof typeof en> = {
	instagram: "channelInstagram",
	x: "channelX",
	facebook: "channelFacebook",
	linkedin: "channelLinkedin",
	threads: "channelThreads",
	bluesky: "channelBluesky",
	tiktok: "channelTiktok",
	pinterest: "channelPinterest",
	youtube: "channelYoutube",
	google_business: "channelGoogleBusiness",
	mastodon: "channelMastodon",
};

/** A network's name as people write it; one this page does not know is shown as sent. */
export const channelName = (channel: string) =>
	CHANNELS[channel] ? t(CHANNELS[channel]) : channel;

/** "Instagram", "Instagram and X", "Instagram, X and Bluesky". */
export function listed(names: string[]): string {
	const last = names.at(-1) ?? "";
	return names.length < 2
		? last
		: `${names.slice(0, -1).join(", ")} and ${last}`;
}

export type Slot = { key: string; channel: string; on: string; topic: string };

/** The Monday on or before `date`. */
const mondayOf = (date: string) => {
	const at = Date.parse(date);
	return new Date(at - ((new Date(at).getUTCDay() + 6) % 7) * DAY_MS)
		.toISOString()
		.slice(0, 10);
};

/** "12 to 18 October", "26 October to 1 November", or a single day. */
function weekRange(from: string, to: string): string {
	const a = parts(from, { day: "numeric", month: "long" });
	const b = parts(to, { day: "numeric", month: "long" });
	if (from === to) return `${a.day} ${a.month}`;
	return a.month === b.month
		? t("marketingDates", { from: a.day ?? "", to: `${b.day} ${b.month}` })
		: t("marketingDates", {
				from: `${a.day} ${a.month}`,
				to: `${b.day} ${b.month}`,
			});
}

/**
 * The slots grouped into the plan's weeks, Monday to Sunday and kept within the plan's days,
 * numbered from its first; a week with no slot is left out.
 */
export function weeks(
	from: string,
	to: string,
	slots: Slot[],
): { n: number; range: string; slots: Slot[] }[] {
	const first = mondayOf(from);
	const grouped = new Map<number, Slot[]>();
	for (const slot of [...slots].sort((a, b) => a.on.localeCompare(b.on))) {
		const n = Math.floor(days(first, slot.on) / 7);
		grouped.set(n, [...(grouped.get(n) ?? []), slot]);
	}
	return [...grouped.entries()]
		.sort(([a], [b]) => a - b)
		.map(([n, inside]) => {
			const monday = Date.parse(first) + n * 7 * DAY_MS;
			const start = new Date(monday).toISOString().slice(0, 10);
			const sunday = new Date(monday + 6 * DAY_MS).toISOString().slice(0, 10);
			return {
				n: n + 1,
				range: weekRange(
					start < from ? from : start,
					sunday > to ? to : sunday,
				),
				slots: inside,
			};
		});
}
