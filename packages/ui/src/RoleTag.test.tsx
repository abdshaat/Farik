import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { RoleTag } from "./RoleTag.tsx";
import type { Role } from "./role.ts";
import { uiStrings } from "./strings.ts";
import { expectNoAxeViolations } from "./test/axe.ts";

const roles: Role[] = [
	"product_manager",
	"scrum_master",
	"architect",
	"software_developer",
	"marketing_specialist",
	"ui_ux_designer",
	"finance_specialist",
];

describe("RoleTag", () => {
	it("names the role in full for screen readers", async () => {
		for (const role of roles) {
			const { container, unmount } = render(<RoleTag role={role} />);
			const abbr = container.querySelector("abbr");
			expect(abbr?.textContent).toBe(uiStrings.roleShort[role]);
			expect(abbr?.getAttribute("title")).toBe(uiStrings.roleName[role]);
			await expectNoAxeViolations(container);
			unmount();
		}
	});

	it("tags_the_designer_ux_in_its_own_colour", () => {
		const designer: Role = "ui_ux_designer";
		const { container } = render(<RoleTag role={designer} />);
		const abbr = container.querySelector("abbr");
		expect(abbr?.textContent).toBe("UX");
		expect(abbr?.getAttribute("title")).toBe("UI/UX Designer");
		const tones = roles.map((role) => {
			const { container: other, unmount } = render(<RoleTag role={role} />);
			const tone = other.querySelector("abbr")?.className;
			unmount();
			return tone;
		});
		expect(new Set(tones).size).toBe(roles.length);
		// And the Designer's class is drawn in the Designer's own token, pale clay.
		const css = readFileSync(
			join(import.meta.dirname, "RoleTag.module.css"),
			"utf8",
		);
		expect(css).toMatch(
			/\.uiUxDesigner \{\s*background: var\(--farik-color-role-ui-ux-designer\);\s*\}/,
		);
	});

	it("role_tag_names_finance", () => {
		const finance: Role = "finance_specialist";
		const { container } = render(<RoleTag role={finance} />);
		const abbr = container.querySelector("abbr");
		expect(abbr?.textContent).toBe("FIN");
		expect(abbr?.getAttribute("title")).toBe("Finance Specialist");
		const tones = roles.map((role) => {
			const { container: other, unmount } = render(<RoleTag role={role} />);
			const tone = other.querySelector("abbr")?.className;
			unmount();
			return tone;
		});
		expect(new Set(tones).size).toBe(roles.length);
		// And its class is drawn in its own token, pale olive.
		const css = readFileSync(
			join(import.meta.dirname, "RoleTag.module.css"),
			"utf8",
		);
		expect(css).toMatch(
			/\.financeSpecialist \{\s*background: var\(--farik-color-role-finance-specialist\);\s*\}/,
		);
	});

	it("says_developer_as_the_mockups_do", () => {
		expect(uiStrings.roleName.software_developer).toBe("Developer");
	});
});
