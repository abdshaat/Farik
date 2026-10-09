import { describe, expect, it } from "vitest";
import {
	afterDays,
	aheadDay,
	amount,
	calendarDay,
	figure,
	pastDay,
	periodWords,
	statusWords,
	todayIso,
} from "./orders.ts";

/** Monday 26 October 2026, 09:15 in the browser's time zone. */
const now = new Date(2026, 9, 26, 9, 15);
/** A time of 2026 in the browser's time zone, as the daemon words it. */
const at = (month: number, day: number, hour = 9) =>
	new Date(2026, month, day, hour).toISOString();

describe("the days an order is said by", () => {
	it("says_a_day_that_has_passed", () => {
		// A time is counted in the browser's own time zone, a day by its date.
		expect(pastDay(at(9, 26), now)).toBe("today");
		expect(pastDay("2026-10-26", now)).toBe("today");
		expect(pastDay(at(9, 25, 23), now)).toBe("yesterday");
		expect(pastDay("2026-10-25", now)).toBe("yesterday");
		expect(pastDay(at(9, 2), now)).toBe("on 2 October");
		expect(pastDay("2026-09-18", now)).toBe("on 18 September");
		// The year is said when it is not this one.
		expect(pastDay("2025-12-31", now)).toBe("on 31 December 2025");
	});

	it("says_a_day_to_come", () => {
		expect(aheadDay("2026-10-26", now)).toBe("today");
		expect(aheadDay(at(9, 26, 20), now)).toBe("today");
		expect(aheadDay("2026-10-27", now)).toBe("tomorrow");
		expect(aheadDay(at(9, 27), now)).toBe("tomorrow");
		expect(aheadDay("2026-11-06", now)).toBe("6 November");
		expect(aheadDay(at(10, 25, 8), now)).toBe("25 November");
		expect(aheadDay("2027-01-05", now)).toBe("5 January 2027");
		// A day that has passed is a date, not "yesterday": nothing is due then.
		expect(aheadDay("2026-10-25", now)).toBe("25 October");
		expect(aheadDay("2026-10-01", now)).toBe("1 October");
	});

	it("says_a_calendar_day_with_its_year_only_when_it_is_not_this_year", () => {
		expect(calendarDay("2026-11-15", now)).toBe("15 November");
		expect(calendarDay("2027-10-26", now)).toBe("26 October 2027");
		expect(calendarDay("2025-01-01", now)).toBe("1 January 2025");
		expect(calendarDay(at(0, 1, 0), now)).toBe("1 January");
	});

	it("finds_today_and_the_day_after_some_days", () => {
		expect(todayIso(now)).toBe("2026-10-26");
		expect(todayIso(new Date(2026, 0, 1, 0, 5))).toBe("2026-01-01");
		expect(todayIso(new Date(2026, 11, 31, 23, 55))).toBe("2026-12-31");
		expect(afterDays("2026-09-20", 30)).toBe("2026-10-20");
		expect(afterDays("2026-12-15", 30)).toBe("2027-01-14");
		expect(afterDays("2028-02-15", 30)).toBe("2028-03-16");
	});
});

describe("the figures and words of an order", () => {
	it("writes_an_amount_with_two_decimals_and_its_currency", () => {
		expect(amount("1450.00", "USD")).toBe("1,450.00 USD");
		expect(amount("2.4", "EUR")).toBe("2.40 EUR");
		expect(amount("10000000.00", "GBP")).toBe("10,000,000.00 GBP");
		expect(figure("0.5")).toBe("0.50");
		expect(figure("96")).toBe("96.00");
	});

	it("words_a_period_and_a_status_and_shows_an_unknown_one_as_sent", () => {
		expect(periodWords("once")).toBe("once");
		expect(periodWords("month")).toBe("a month");
		expect(periodWords("year")).toBe("a year");
		expect(periodWords("week")).toBe("week");
		expect(statusWords("preparing")).toBe("Being prepared");
		expect(statusWords("shipped")).toBe("Shipped");
		expect(statusWords("delayed")).toBe("Delayed");
		expect(statusWords("problem")).toBe("A problem");
		expect(statusWords("lost")).toBe("lost");
	});
});
