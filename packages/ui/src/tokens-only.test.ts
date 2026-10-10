import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const here = import.meta.dirname;
const colour =
	/#[0-9a-f]{3,8}\b|rgb\(|hsl\(|(?<![\w-])(white|black|red|blue|green|gray|grey)(?![\w-])/i;

const files = readdirSync(here, { recursive: true, encoding: "utf8" }).filter(
	(f) => f.endsWith(".css"),
);
const read = (f: string) => readFileSync(join(here, f), "utf8");

describe("the stylesheets", () => {
	it("uses no colour but the tokens", () => {
		const offenders = files.filter((f) => colour.test(read(f)));
		expect(offenders).toEqual([]);
	});

	it("aligns text by logical sides only", () => {
		const offenders = files.filter((f) =>
			/text-align:\s*(left|right)/.test(read(f)),
		);
		expect(offenders).toEqual([]);
	});

	// No component leans on the page's font: each root rule (the first in its
	// file) sets a family token. An avatar has no text, and a status word
	// takes the font of the sentence it sits in.
	it("sets a font family token on every component's root", () => {
		const offenders = files
			.filter(
				(f) => !["Avatar.module.css", "StatusWord.module.css"].includes(f),
			)
			.filter(
				(f) =>
					!/var\(--catervas-type-[a-z]+-family\)/.test(
						read(f).split("}")[0] ?? "",
					),
			);
		expect(offenders).toEqual([]);
	});
});
