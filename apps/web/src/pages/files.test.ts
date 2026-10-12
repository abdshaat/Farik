import { describe, expect, it } from "vitest";
import { timeAgo } from "./files.ts";

const NOW = new Date("2026-10-10T12:00:00Z");
const before = (seconds: number) =>
	new Date(NOW.getTime() - seconds * 1000).toISOString();

describe("timeAgo", () => {
	it("says how long ago in words", () => {
		expect(timeAgo(before(30), NOW)).toBe("just now");
		expect(timeAgo(before(5 * 60), NOW)).toBe("5 minutes ago");
		expect(timeAgo(before(3 * 3600), NOW)).toBe("3 hours ago");
		expect(timeAgo(before(86400), NOW)).toBe("yesterday");
		expect(timeAgo(before(5 * 86400), NOW)).toBe("5 days ago");
		expect(timeAgo(before(45 * 86400), NOW)).toBe("last month");
		expect(timeAgo(before(400 * 86400), NOW)).toBe("last year");
	});
});
