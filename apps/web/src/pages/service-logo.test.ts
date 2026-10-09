import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { serviceLogo } from "./service-logo.ts";

const roles = join(import.meta.dirname, "../../../../crates/roles/roles");

describe("serviceLogo", () => {
	it("has a logo for every connector a shipped kit offers", () => {
		const names = readdirSync(roles, { withFileTypes: true })
			.filter((d) => d.isDirectory())
			.flatMap((d) => {
				try {
					const yaml = readFileSync(join(roles, d.name, "kit.yaml"), "utf8");
					return [...yaml.matchAll(/^ {2}- name: ([a-z0-9-]+)$/gm)].map(
						(m) => m[1] as string,
					);
				} catch {
					return [];
				}
			});
		expect(names.length).toBeGreaterThan(0);
		const missing = names.filter((n) => serviceLogo(n) === serviceLogo("plug"));
		expect(missing).toEqual([]);
	});

	it("falls back to the plug for a name it does not know", () => {
		expect(serviceLogo("airtable")).toBe(serviceLogo("plug"));
	});

	it("has a logo for the built-in Playwright", () => {
		expect(serviceLogo("playwright")).not.toBe(serviceLogo("plug"));
	});
});
