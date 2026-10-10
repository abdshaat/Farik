import { expectNoAxeViolations } from "@catervas/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../../strings/en.ts";
import { BUDGET_ROW, PLAN_BUDGET_ROW, RUNNING_PLAN } from "../../test/ads.ts";
import type { FakeSocket } from "../../test/fake-socket.ts";
import { TEAM, TITLE } from "../../test/marketing.ts";
import { todayWith } from "../../test/posts.ts";
import { answerQuery } from "../../test/render-app.tsx";

/** Today with `row` waiting; its "Raise the budget" opened, and the plan it asks for answered. */
async function opened(row: object = BUDGET_ROW) {
	const { container, s } = await todayWith({ waiting: [row], team: TEAM });
	const list = await screen.findByRole("list", { name: en.waitingList });
	fireEvent.click(
		within(list).getByRole("button", { name: "Raise the budget" }),
	);
	await answerQuery(s, "marketing_plan.get", RUNNING_PLAN);
	const dialog = await screen.findByRole("dialog", {
		name: `Raise the budget of ${TITLE}`,
	});
	return { container, s: s as FakeSocket, dialog };
}

/** The raise the dialog sent, once it has. */
const raised = (s: FakeSocket) =>
	waitFor(() => {
		const call = s.calls("marketing_budget.raise")[0];
		if (!call) throw new Error("no raise was sent");
		return call;
	});

const type = (dialog: HTMLElement, label: string, value: string) =>
	fireEvent.change(within(dialog).getByLabelText(label), {
		target: { value },
	});

describe("the raise of a marketing budget", () => {
	afterEach(() => vi.unstubAllGlobals());

	it("raise_sends_the_raised_budget", async () => {
		const { container, s, dialog } = await opened();

		// What reached its budget, who writes the new version, and what happens after.
		expect(dialog.textContent).toContain(
			"Bakery near me reached its budget. Choose the new budgets, and Kai writes a new version of the plan with them for you to approve.",
		);
		expect(within(dialog).getByLabelText("Google Ads budget")).toBeTruthy();
		expect(dialog.textContent).toContain(
			"For the whole plan. Now $450.00 USD, of which $318.65 is spent.",
		);
		// Only the campaign that reached its own budget has a field; the other keeps its budget.
		expect(within(dialog).getByLabelText("Bakery near me")).toBeTruthy();
		expect(dialog.textContent).toContain("Now $150.00 USD, all of it spent.");
		expect(within(dialog).queryByLabelText(/Pie pre-orders/)).toBeNull();
		for (const words of [
			"Kai writes the new version at once, from today to Sunday 22 November: it does not wait for a sprint.",
			"It waits for you on Today, like any plan. Bakery near me stays paused until you approve it.",
			"Once you approve it, Kai raises the budget at Google and starts Bakery near me again.",
		])
			expect(within(dialog).getByText(words)).toBeTruthy();
		await expectNoAxeViolations(container);

		// The plan's total rises by what the Google Ads budget does.
		type(dialog, "Google Ads budget", "500.00");
		type(dialog, "Bakery near me", "200.00");
		expect(dialog.textContent).toContain(
			"The plan’s total rises by the same amount, from $450.00 to $500.00 USD.",
		);
		fireEvent.click(
			within(dialog).getByRole("button", {
				name: "Ask Kai for the new version",
			}),
		);
		const call = await raised(s);
		expect(call.params).toEqual({
			plan: "MP-3",
			google_ads: "500.00",
			campaigns: [{ key: "near-me", budget: "200.00" }],
		});
		await s.reply(call, { task_id: "CTV-40" });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("an_amount_the_raise_would_refuse_is_refused_in_the_dialog", async () => {
		const { container, s, dialog } = await opened();
		const ask = within(dialog).getByRole("button", {
			name: "Ask Kai for the new version",
		});

		// At what is spent: nothing is sent, and the field says what to change.
		type(dialog, "Google Ads budget", "500.00");
		type(dialog, "Bakery near me", "150.00");
		fireEvent.click(ask);
		const field = within(dialog).getByLabelText("Bakery near me");
		expect(field.getAttribute("aria-invalid")).toBe("true");
		expect(dialog.textContent).toContain(
			"Make it more than the $150.00 USD already spent.",
		);
		expect(
			within(dialog)
				.getByLabelText("Google Ads budget")
				.getAttribute("aria-invalid"),
		).toBeNull();
		await expectNoAxeViolations(container);

		// Campaigns that add up to more than the Google Ads budget: 300.00 and 200.00 are 500.00.
		type(dialog, "Bakery near me", "200.00");
		type(dialog, "Google Ads budget", "480.00");
		fireEvent.click(ask);
		expect(
			within(dialog)
				.getByLabelText("Google Ads budget")
				.getAttribute("aria-invalid"),
		).toBe("true");
		expect(dialog.textContent).toContain(
			"The campaigns’ budgets add up to $500.00 USD: make this at least that.",
		);
		expect(field.getAttribute("aria-invalid")).toBeNull();

		// Not at or below what is spent, and not below what the plan has now.
		type(dialog, "Google Ads budget", "318.65");
		fireEvent.click(ask);
		expect(dialog.textContent).toContain(
			"Make it more than the $318.65 USD already spent.",
		);
		type(dialog, "Google Ads budget", "400.00");
		fireEvent.click(ask);
		expect(dialog.textContent).toContain(
			"Make it at least the $450.00 USD it is now.",
		);

		// Something that is not an amount.
		type(dialog, "Google Ads budget", "lots");
		fireEvent.click(ask);
		expect(dialog.textContent).toContain("Type an amount, like 500.00.");
		expect(s.calls("marketing_budget.raise")).toHaveLength(0);
	});

	it("the_plan_s_own_cap_asks_for_the_google_ads_budget_alone", async () => {
		const { s, dialog } = await opened(PLAN_BUDGET_ROW);
		expect(dialog.textContent).toContain(
			"Its ads reached their budget. Choose the new budgets, and Kai writes a new version of the plan with them for you to approve.",
		);
		expect(
			within(dialog).getByText(
				"It waits for you on Today, like any plan. Its ads stay paused until you approve it.",
			),
		).toBeTruthy();
		expect(
			within(dialog).getByText(
				"Once you approve it, Kai raises the budget at Google and starts its ads again.",
			),
		).toBeTruthy();
		expect(within(dialog).getAllByRole("textbox")).toHaveLength(1);
		type(dialog, "Google Ads budget", "600");
		fireEvent.click(
			within(dialog).getByRole("button", {
				name: "Ask Kai for the new version",
			}),
		);
		// An amount typed without cents is sent with them.
		const call = await raised(s);
		expect(call.params).toEqual({
			plan: "MP-3",
			google_ads: "600.00",
			campaigns: [],
		});
	});

	it("a_refused_raise_says_why_and_keeps_the_dialog", async () => {
		const { s, dialog } = await opened();
		type(dialog, "Google Ads budget", "500.00");
		type(dialog, "Bakery near me", "200.00");
		fireEvent.click(
			within(dialog).getByRole("button", {
				name: "Ask Kai for the new version",
			}),
		);
		const call = await raised(s);
		await s.fail(call, -32000, "refused", {
			errors: [
				{ path: "/plan", message: "A raise is open.", code: "raise_open" },
			],
		});
		expect((await within(dialog).findByRole("alert")).textContent).toBe(
			en.refuseRaiseOpen,
		);
		expect(screen.getByRole("dialog")).toBeTruthy();
	});
});
