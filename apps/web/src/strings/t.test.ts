import { describe, expect, it } from "vitest";
import { t } from "./t.ts";

describe("t", () => {
	it("fills_each_placeholder_with_the_words_as_given", () => {
		// Agent-written words are text: no `$` pattern, and no placeholder of their own, is read.
		expect(t("gateTitle", { title: "Price $& and $$5" })).toBe(
			"Accept Price $& and $$5",
		);
		expect(
			t("waitingAcceptance", { title: "Say {agent} twice", agent: "Theo" }),
		).toBe("Accept Say {agent} twice");
		expect(t("triesOf", { try: 1, of: 4 })).toBe("1 of 4");
	});
});
