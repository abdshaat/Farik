import { AVATAR_KEYS } from "@catervas/brand";
import { describe, expect, it } from "vitest";
import { AVATAR_URLS } from "./avatars.ts";

describe("AVATAR_URLS", () => {
	it("gives each avatar key its own picture", () => {
		expect(AVATAR_KEYS.length).toBeGreaterThan(0);
		for (const key of AVATAR_KEYS) {
			expect(AVATAR_URLS[key], key).toMatch(new RegExp(`/${key}-256\\.png$`));
		}
	});
});
