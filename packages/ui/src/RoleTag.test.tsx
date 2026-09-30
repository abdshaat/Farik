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

	it("says_developer_as_the_mockups_do", () => {
		expect(uiStrings.roleName.software_developer).toBe("Developer");
	});
});
