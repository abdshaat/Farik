import { expectNoAxeViolations } from "@catervas/ui/test";
import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import { sentCommand } from "../test/gate.ts";
import { IN_REVIEW, todayWithPlan } from "../test/productPlan.ts";
import { bodyOf, refusedBy } from "../test/schema.ts";
import cardCss from "./ProductPlanCard.module.css";
import { projectName, writtenWhen } from "./ProductPlanCard.tsx";

const card = () =>
	screen.findByRole("region", { name: "Your product plan to approve" });

describe("the product plan on Today", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("shows_the_plan_in_full", async () => {
		const { container } = await todayWithPlan({});
		const region = await card();
		expect(
			within(region).getByText(
				"From Mira, written from your chat this morning",
			),
		).toBeTruthy();
		expect(
			within(region).getByText(
				"Here is what we will build and in what order. Read both parts, then approve it or send it back with what to change.",
			),
		).toBeTruthy();
		const spec = within(region).getByText("Product description");
		const roadmap = within(region).getByText("Roadmap");
		expect(
			spec.compareDocumentPosition(roadmap) & Node.DOCUMENT_POSITION_FOLLOWING,
		).toBeTruthy();
		// Each document as rendered Markdown, not as its marks.
		for (const heading of ["Little Oak Cakes", "Who it is for", "Now", "Later"])
			expect(
				within(region).getByRole("heading", { name: heading }),
			).toBeTruthy();
		expect(
			within(region)
				.getAllByRole("listitem")
				.map((item) => item.textContent),
		).toContain(
			"Customers choose a size, a flavour and a collection day, and pay a 30% deposit.",
		);
		expect(region.textContent).not.toContain("## ");
		expect(
			within(region).getByText(
				"Nothing else is planned until you approve. Then your team documents the plan in a first sprint.",
			),
		).toBeTruthy();
		expect(
			within(region).getByRole("textbox", { name: /Notes for Mira/ }),
		).toBeTruthy();
		await expectNoAxeViolations(container);
	});

	it("the_product_plan_card_frames_html", async () => {
		const at = (n: string) => `docs/catervas/product/${n}.html`;
		const { container } = await todayWithPlan({
			state: {
				...IN_REVIEW,
				documents: [
					{ path: at("spec"), text: "<h1>Secret spec</h1>" },
					{ path: at("roadmap"), text: "<h1>Secret roadmap</h1>" },
				],
			},
		});
		const region = await card();
		const frames = Array.from(region.querySelectorAll("iframe"));
		expect(frames.map((f) => f.getAttribute("src"))).toEqual([
			`/documents/raw?path=${encodeURIComponent(at("spec"))}&task=CTV-2`,
			`/documents/raw?path=${encodeURIComponent(at("roadmap"))}&task=CTV-2`,
		]);
		expect(frames.map((f) => f.getAttribute("title"))).toEqual([
			"Product description",
			"Roadmap",
		]);
		expect(frames.every((f) => f.getAttribute("sandbox") === "")).toBe(true);
		expect(container.innerHTML).not.toContain("Secret");
		for (const f of frames) expect(f.className).toContain(cardCss.frame);
	});

	it("approve_accepts_the_result", async () => {
		const { s } = await todayWithPlan({});
		const region = await card();
		fireEvent.click(within(region).getByRole("button", { name: "Approve" }));
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "human_accept",
				body: { task_id: "CTV-2", subject: "result" },
			},
		});
		expect(refusedBy("humanAcceptBody", bodyOf(sent))).toEqual([]);
	});

	it("send_back_needs_notes", async () => {
		const { s } = await todayWithPlan({});
		const region = await card();
		fireEvent.click(within(region).getByRole("button", { name: "Send back" }));
		expect(
			await within(region).findByText("Write what Mira should change first."),
		).toBeTruthy();
		expect(s.calls("command")).toHaveLength(0);

		fireEvent.change(within(region).getByRole("textbox"), {
			target: { value: "Add gift vouchers to Next" },
		});
		fireEvent.click(within(region).getByRole("button", { name: "Send back" }));
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "human_send_back",
				body: {
					task_id: "CTV-2",
					subject: "result",
					message: "Add gift vouchers to Next",
					failed_criteria: [],
				},
			},
		});
		expect(refusedBy("humanSendBackBody", bodyOf(sent))).toEqual([]);
	});

	it("says_what_approving_does_under_the_policy", async () => {
		for (const [integration, words] of [
			["pull_request", en.docChangesPullRequest],
			["manual", en.docChangesManual],
			["auto_merge", undefined],
		] as const) {
			await todayWithPlan({ integration });
			const region = await card();
			if (words) expect(within(region).getByText(words)).toBeTruthy();
			else expect(region.textContent).not.toContain("Nothing changes");
			cleanup();
		}
	});

	it("says_when_it_was_written", () => {
		const now = new Date(2026, 9, 10, 20, 0);
		const at = (day: number, hour: number, minute = 0) =>
			new Date(2026, 9, day, hour, minute).toISOString();
		expect(writtenWhen(at(10, 9, 30), now)).toBe("this morning");
		expect(writtenWhen(at(10, 14), now)).toBe("this afternoon");
		expect(writtenWhen(at(10, 19), now)).toBe("this evening");
		expect(writtenWhen(at(9, 23), now)).toBe("yesterday");
		expect(writtenWhen(at(7, 9), now)).toBe("on Wednesday");
		expect(projectName("little-oak-cakes")).toBe("Little Oak Cakes");
	});
});
