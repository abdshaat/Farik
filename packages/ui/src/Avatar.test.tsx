import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Avatar } from "./Avatar.tsx";
import { AVATAR_URLS } from "./avatars.ts";
import { expectNoAxeViolations } from "./test/axe.ts";

describe("Avatar", () => {
	it("shows the agent's character with its name", async () => {
		const { container } = render(
			<Avatar avatarKey="architect" name="Ada" size={64} />,
		);
		const img = screen.getByRole("img", { name: "Ada" });
		expect(img.getAttribute("src")).toBe(AVATAR_URLS.architect);
		expect(img.getAttribute("width")).toBe("64");
		expect(img.getAttribute("height")).toBe("64");
		await expectNoAxeViolations(container);
	});
});
