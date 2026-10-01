import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ChatList } from "./ChatList.tsx";
import { expectNoAxeViolations } from "./test/axe.ts";

describe("ChatList", () => {
	it("shows who said what, and when", async () => {
		const { container } = render(
			<ChatList
				label="Channel"
				messages={[
					{
						id: "1",
						author: { name: "Ada", role: "architect", avatarKey: "architect" },
						time: "09:15",
						text: "Line one\nLine two",
						thread: "Sprint 2",
					},
					{ id: "2", author: { name: "Founder" }, time: "09:16", text: "ok" },
				]}
			/>,
		);
		const [first, second] = within(
			screen.getByRole("list", { name: "Channel" }),
		).getAllByRole("listitem");
		expect(within(first as HTMLElement).getByText("Ada")).toBeTruthy();
		expect(within(first as HTMLElement).getByText("09:15")).toBeTruthy();
		// A free-form time has no machine-readable dateTime, so it is no <time>.
		expect(container.querySelector("time")).toBeNull();
		expect(first?.textContent).toContain("Line one\nLine two");
		expect(first?.textContent).toContain("Sprint 2");
		expect(within(first as HTMLElement).getByAltText("Ada")).toBeTruthy();
		expect(within(second as HTMLElement).queryByRole("img")).toBeNull();
		await expectNoAxeViolations(container);
	});

	it("shows an agent's text as text", () => {
		const { container } = render(
			<ChatList
				label="Channel"
				messages={[
					{
						id: "1",
						author: { name: "Ada" },
						time: "09:15",
						text: "<img src=x onerror=alert(1)>",
					},
				]}
			/>,
		);
		expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeTruthy();
		expect(container.querySelector("img")).toBeNull();
	});
});
