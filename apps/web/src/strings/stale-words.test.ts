import { describe, expect, it } from "vitest";
import { en } from "./en.ts";

/** Words that described the product before only code was reviewed and chats took work (ADRs 0061, 0063). */
const STALE = [
	"once {pm} agrees",
	"suggests a request, and you decide",
	"asks before writing the plan",
	"Someone on the team reviewed it",
	"approve and accept",
	"reviews the work before you see it",
	"Another agent checks it",
	"checks the plan before you read it",
	"have their notes checked",
	"a teammate reviews the work",
	"You accept the work",
];

describe("the strings", () => {
	it("says_nothing_the_product_no_longer_does", () => {
		const found = Object.entries(en).flatMap(([key, value]) =>
			STALE.filter((phrase) => String(value).includes(phrase)).map(
				(phrase) => `${key}: ${phrase}`,
			),
		);
		expect(found).toEqual([]);
	});
});
