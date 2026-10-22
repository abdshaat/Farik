import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = join(import.meta.dirname, "..");

/** Every non-test source file under `dir`. */
function sources(dir: string): string[] {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		if (statSync(path).isDirectory()) return sources(path);
		return /\.tsx?$/.test(name) && !/\.test\./.test(name) ? [path] : [];
	});
}

describe("links", () => {
	it("keeps_every_team_link_inside_its_team", () => {
		// A page of a team reaches `Link`, `NavLink`, `Navigate` and `useNavigate` through
		// `app/teams.tsx`, which puts `/t/<team>` before an absolute address; the router's own would
		// leave the team. Setup, finding projects, All teams and This computer are not a team's.
		const files = [
			...sources(join(SRC, "pages")),
			...sources(join(SRC, "shell")),
			...sources(join(SRC, "components")),
		].filter(
			(file) =>
				!relative(SRC, file).startsWith(join("pages", "setup")) &&
				!relative(SRC, file).startsWith(join("pages", "find")) &&
				!/(AllTeams|ThisComputer)\.tsx$/.test(file),
		);
		const router = /import\s*(type\s*)?\{([^}]*)\}\s*from\s*"react-router"/g;
		const leaks = files.flatMap((file) =>
			[...readFileSync(file, "utf8").matchAll(router)]
				.filter((m) =>
					/\b(Link|NavLink|Navigate|useNavigate)\b/.test(m[2] ?? ""),
				)
				.map(() => relative(SRC, file)),
		);
		expect(leaks).toEqual([]);
	});
});
