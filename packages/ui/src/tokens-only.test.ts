import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const here = import.meta.dirname;
const colour =
	/#[0-9a-f]{3,8}\b|rgb\(|hsl\(|(?<![\w-])(white|black|red|blue|green|gray|grey)(?![\w-])/i;

describe("the stylesheets", () => {
	it("uses no colour but the tokens", () => {
		const files = readdirSync(here, {
			recursive: true,
			encoding: "utf8",
		}).filter((f) => f.endsWith(".css"));
		const offenders = files.filter((f) =>
			colour.test(readFileSync(join(here, f), "utf8")),
		);
		expect(offenders).toEqual([]);
	});
});
