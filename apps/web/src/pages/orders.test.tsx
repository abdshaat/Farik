import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { sentCommand } from "../test/gate.ts";
import { at, EFFECTIVE, MARKUP, NOW, ORDERS, TEAM } from "../test/orders.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
import { SITES } from "../test/sites.ts";

/** One agent's page, its orders answered unless `orders` is left out. */
async function opened(agent: string, orders: object | null = ORDERS) {
	vi.useFakeTimers({ toFake: ["Date"] });
	vi.setSystemTime(NOW);
	const { container, socket } = await renderApp(`/team/${agent}`);
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", {
		team: TEAM,
		agents: EFFECTIVE,
		judges: { auto: null, architect: null, scrum_master: null },
		max_agents: 7,
		connectors: [],
		sandboxed: true,
	});
	await answerQuery(s, "models.list", { models: [] });
	await answerQuery(s, "skills.list", { skills: [] });
	if (agent === "ivo") await answerQuery(s, "sites.list", SITES);
	if (orders) await answerQuery(s, "purchase_orders.list", orders);
	await screen.findByRole("heading", {
		name:
			agent === "ivo"
				? "Ivo, your Procurement Specialist"
				: "Theo, your Developer",
	});
	return { container, s };
}

const asked = (s: FakeSocket) =>
	s.calls("query").filter((q) => q.params.name === "purchase_orders.list")
		.length;

/** The list item of one order in the list named `list`. */
async function itemOf(list: string, order: string) {
	const found = within(await screen.findByRole("list", { name: list }))
		.getAllByRole("listitem")
		.find((one) => within(one).queryByText(`PO-${order}`));
	if (!found) throw new Error(`no PO-${order} in ${list}`);
	return found;
}

describe("the orders on the Procurement Specialist's page", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("lists_orders_to_place_and_placed_with_their_status", async () => {
		const { container } = await opened("ivo");
		await expectNoAxeViolations(container);

		// Orders come directly before the sites the agent may read.
		const heading = await screen.findByRole("heading", { name: "Orders" });
		const section = heading.closest("section") as HTMLElement;
		expect(section.nextElementSibling).toBe(
			screen.getByRole("heading", { name: en.sitesTitle }).closest("section"),
		);
		expect(section.textContent).toContain(
			"Ivo suggests orders and follows them up. Farik never orders or pays: you place each order yourself, then mark it placed and, when it comes, received.",
		);

		// An approved order waits for the owner to place it, with the day it closes by itself.
		const toPlace = await itemOf(en.ordersToPlace, "12");
		expect(toPlace.textContent).toContain("Pie Box Pros");
		expect(toPlace.textContent).toContain("1,450.00 USD once");
		expect(toPlace.textContent).toContain("Approved today.");
		expect(toPlace.textContent).toContain(
			"If you don’t mark it placed by 25 November, it closes by itself.",
		);
		expect(
			within(toPlace).getByRole("button", { name: "Mark placed" }),
		).toBeTruthy();
		expect(
			within(toPlace).getByRole("button", { name: "Download PO-12.xlsx" }),
		).toBeTruthy();

		// A placed order shows what the agent learned, in the agent's words, as text in a frame.
		const delayed = await itemOf(en.ordersPlacedTitle, "11");
		expect(delayed.textContent).toContain("Northfield Mill");
		expect(delayed.textContent).toContain("212.40 USD once");
		expect(delayed.textContent).toContain(
			"Placed on 2 October, paid 190.00 EUR",
		);
		expect(within(delayed).getByText("Delayed")).toBeTruthy();
		expect(delayed.textContent).toContain("Ivo, from a follow-up yesterday:");
		const note = within(delayed).getByText(/The mill’s order page says flour/);
		expect(note.getAttribute("data-trust")).toBe("untrusted");
		expect(note.textContent).toContain(MARKUP);
		expect(delayed.querySelector("b")).toBeNull();
		expect(delayed.textContent).toContain("Expected 30 October.");
		expect(within(delayed).queryByText("Overdue")).toBeNull();
		expect(
			within(delayed)
				.getAllByRole("button")
				.map((button) => button.textContent),
		).toEqual([
			"Mark received",
			"Correct the status",
			"Ask for a follow-up",
			"It won’t come",
		]);

		// The owner's correction is the owner's own words, plain; one past its day says Overdue.
		const overdue = await itemOf(en.ordersPlacedTitle, "10");
		expect(overdue.textContent).toContain("Placed on 18 September");
		expect(overdue.textContent).not.toContain("paid");
		expect(within(overdue).getByText("Shipped")).toBeTruthy();
		expect(overdue.textContent).toContain("You corrected it on 28 September:");
		const said = within(overdue).getByText(
			"They emailed me: it left their warehouse on 26 September.",
		);
		expect(said.getAttribute("data-trust")).toBeNull();
		expect(within(overdue).getByText("Overdue")).toBeTruthy();
		expect(overdue.textContent).toContain("Expected 1 October.");

		// With no news yet, it is overdue 30 days after it was placed.
		const quiet = await itemOf(en.ordersPlacedTitle, "13");
		expect(within(quiet).getByText("No news yet.")).toBeTruthy();
		expect(within(quiet).getByText("Overdue")).toBeTruthy();
		expect(quiet.textContent).toContain("Expected 20 October.");
	});

	it("says_there_is_nothing_to_place_or_receive", async () => {
		await opened("ivo", { orders: [ORDERS.orders[0]] });
		expect(
			await screen.findByText("Nothing to place or receive."),
		).toBeTruthy();
		expect(
			screen.queryByRole("heading", { name: en.ordersToPlace }),
		).toBeNull();
		expect(
			screen.queryByRole("heading", { name: en.ordersPlacedTitle }),
		).toBeNull();
	});

	it("orders_that_are_not_over_are_not_recent", async () => {
		const only = (...numbers: number[]) => ({
			orders: ORDERS.orders.filter((one) => numbers.includes(one.order)),
		});
		// One order has ended, two have not: only the one that has is a recent one.
		await opened("ivo", only(9, 12, 11));
		const recent = within(
			await screen.findByRole("list", { name: "Recent orders" }),
		).getAllByRole("listitem");
		expect(recent.map((one) => one.textContent)).toEqual([
			"PO-9Pie Tin SupplyReceived on 30 September, paid 88.50 EUR",
		]);
		// Something to place and something to receive: nothing says there is nothing.
		expect(screen.queryByText("Nothing to place or receive.")).toBeNull();
	});

	it("an_order_to_place_alone_is_something_to_do", async () => {
		await opened("ivo", {
			orders: ORDERS.orders.filter((one) => one.order === 12),
		});
		expect(
			await screen.findByRole("heading", { name: en.ordersToPlace }),
		).toBeTruthy();
		expect(screen.queryByText("Nothing to place or receive.")).toBeNull();
	});

	it("a_placed_order_alone_is_something_to_do", async () => {
		await opened("ivo", {
			orders: ORDERS.orders.filter((one) => one.order === 11),
		});
		expect(
			await screen.findByRole("heading", { name: en.ordersPlacedTitle }),
		).toBeTruthy();
		expect(screen.queryByText("Nothing to place or receive.")).toBeNull();
	});

	it("recent_orders_say_how_each_ended", async () => {
		await opened("ivo");
		const recent = within(
			await screen.findByRole("list", { name: "Recent orders" }),
		)
			.getAllByRole("listitem")
			.map((one) => one.textContent);

		// The five that ended last, newest first; the sixth is left out.
		expect(recent).toEqual([
			"PO-5Cog & CoYou closed it on 12 October: it did not come.",
			"PO-9Pie Tin SupplyReceived on 30 September, paid 88.50 EUR",
			"PO-8Box & Bag CoYou rejected it on 25 September.",
			`PO-7Bake ${MARKUP} CoClosed by itself on 21 September: not marked placed within 30 days of your approval.`,
			"PO-6Bake Supply CoClosed by itself on 20 September: not decided within 30 days.",
		]);
		expect(screen.queryByText("Old Mill")).toBeNull();
	});

	it("mark_placed_sends_then_asks_for_the_follow_up", async () => {
		const { container, s } = await opened("ivo");
		const open = async () => {
			fireEvent.click(
				within(await itemOf(en.ordersToPlace, "12")).getByRole("button", {
					name: "Mark placed",
				}),
			);
			return screen.findByRole("dialog", {
				name: "Mark PO-12 from Pie Box Pros placed?",
			});
		};
		const dialog = await open();

		// Today, nothing paid yet, the order's currency, and the request ticked in the owner's words.
		const day = within(dialog).getByLabelText(
			"When did you place it?",
		) as HTMLInputElement;
		expect(day.value).toBe("2026-10-26");
		expect(
			(within(dialog).getByLabelText(/What did you pay/) as HTMLInputElement)
				.value,
		).toBe("");
		expect(
			(within(dialog).getByLabelText("Currency") as HTMLInputElement).value,
		).toBe("USD");
		expect(
			within(dialog).getByText(
				"Leave it empty if you don’t know yet; you can add it when it comes.",
			),
		).toBeTruthy();
		const ask = within(dialog).getByRole("checkbox", {
			name: "Ask Ivo to follow up until it arrives",
		}) as HTMLInputElement;
		expect(ask.checked).toBe(true);
		const request = () =>
			within(dialog).getByRole("textbox", {
				name: "Your request",
			}) as HTMLTextAreaElement;
		expect(request().value).toBe(
			"Follow up on PO-12 from Pie Box Pros until it arrives. I placed it on 26 October.",
		);
		expect(
			within(dialog).getByText(
				"It goes to the team as your request, in your words. Change them if you like.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		// Closing sends nothing.
		fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);

		// Unticked, only the order is marked placed; the request goes nowhere.
		const bare = await open();
		fireEvent.click(
			within(bare).getByRole("checkbox", {
				name: "Ask Ivo to follow up until it arrives",
			}),
		);
		expect(
			within(bare).queryByRole("textbox", { name: "Your request" }),
		).toBeNull();
		fireEvent.click(within(bare).getByRole("button", { name: "Mark placed" }));
		const first = await sentCommand(s);
		expect(first.params).toEqual({
			command: {
				command: "purchase_order_place",
				body: { order: 12, placed_on: "2026-10-26" },
			},
		});
		const before = asked(s);
		await s.reply(first, { said: "marked PO-12 placed", events: [92] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("request.file")).toHaveLength(0);
		await waitFor(() => expect(asked(s)).toBe(before + 1));
	});

	it("mark_placed_files_the_edited_request_after_the_order_is_marked", async () => {
		const { s } = await opened("ivo");
		fireEvent.click(
			within(await itemOf(en.ordersToPlace, "12")).getByRole("button", {
				name: "Mark placed",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Mark PO-12 from Pie Box Pros placed?",
		});
		fireEvent.change(within(dialog).getByLabelText("When did you place it?"), {
			target: { value: "2026-10-25" },
		});
		fireEvent.change(within(dialog).getByLabelText(/What did you pay/), {
			target: { value: " 1450 " },
		});
		fireEvent.change(within(dialog).getByLabelText("Currency"), {
			target: { value: "eur" },
		});
		// The draft follows the day until the owner writes their own words.
		const request = () =>
			within(dialog).getByRole("textbox", {
				name: "Your request",
			}) as HTMLTextAreaElement;
		expect(request().value).toBe(
			"Follow up on PO-12 from Pie Box Pros until it arrives. I placed it on 25 October.",
		);
		expect(
			(within(dialog).getByLabelText("Currency") as HTMLInputElement).value,
		).toBe("EUR");
		fireEvent.change(request(), {
			target: { value: "Please follow up on PO-12 and tell me if it is late." },
		});
		fireEvent.change(within(dialog).getByLabelText("When did you place it?"), {
			target: { value: "2026-10-24" },
		});
		expect(request().value).toBe(
			"Please follow up on PO-12 and tell me if it is late.",
		);

		// A refused order files no request, and says why in words.
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Mark placed" }),
		);
		const refused = await sentCommand(s);
		expect(refused.params).toEqual({
			command: {
				command: "purchase_order_place",
				body: {
					order: 12,
					placed_on: "2026-10-24",
					paid: "1450",
					currency: "EUR",
				},
			},
		});
		await s.reply(refused, {
			error: {
				kind: "refused",
				detail: "purchase_order_placed: order 12 was placed already",
			},
		});
		expect((await within(dialog).findByRole("alert")).textContent).toBe(
			en.refusePurchaseOrderPlaced,
		);
		expect(s.calls("request.file")).toHaveLength(0);

		// Marked, then the request in the owner's own words; the dialog closes when both are done.
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Mark placed" }),
		);
		const marked = await sentCommand(s, 2);
		await s.reply(marked, { said: "marked PO-12 placed", events: [93] });
		const filed = await waitFor(() => {
			const frames = s.calls("request.file");
			if (frames.length === 0) throw new Error("no request was filed");
			return frames[0];
		});
		expect((filed as { params: object }).params).toEqual({
			text: "Please follow up on PO-12 and tell me if it is late.",
		});
		// A request that is refused leaves the order marked: only the request is asked again.
		await s.fail(filed as never, -32002, "refused", {
			errors: [{ path: "", message: "too short", code: "too_short" }],
		});
		expect((await within(dialog).findByRole("alert")).textContent).toBe(
			en.requestTooShort,
		);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Mark placed" }),
		);
		const again = await waitFor(() => {
			const frames = s.calls("request.file");
			if (frames.length < 2) throw new Error("not filed again");
			return frames[1];
		});
		expect(s.calls("command")).toHaveLength(2);
		await s.reply(again as never, { task_id: "FRK-50" });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("mark_placed_with_no_day_and_the_request_in_the_owners_words", async () => {
		const { s } = await opened("ivo");
		fireEvent.click(
			within(await itemOf(en.ordersToPlace, "12")).getByRole("button", {
				name: "Mark placed",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Mark PO-12 from Pie Box Pros placed?",
		});
		const request = () =>
			within(dialog).getByRole("textbox", {
				name: "Your request",
			}) as HTMLTextAreaElement;
		const mark = () =>
			within(dialog).getByRole("button", {
				name: "Mark placed",
			}) as HTMLButtonElement;

		// With no day, the daemon takes today, and the request says today.
		fireEvent.change(within(dialog).getByLabelText("When did you place it?"), {
			target: { value: "" },
		});
		expect(request().value).toBe(
			"Follow up on PO-12 from Pie Box Pros until it arrives. I placed it on 26 October.",
		);

		// A request with no words cannot be sent; unticked, there is none to send.
		fireEvent.change(request(), { target: { value: "  " } });
		expect(mark().disabled).toBe(true);
		fireEvent.click(
			within(dialog).getByRole("checkbox", {
				name: "Ask Ivo to follow up until it arrives",
			}),
		);
		expect(mark().disabled).toBe(false);
		fireEvent.click(
			within(dialog).getByRole("checkbox", {
				name: "Ask Ivo to follow up until it arrives",
			}),
		);
		expect(mark().disabled).toBe(true);

		// What is sent is the owner's words, trimmed.
		fireEvent.change(request(), {
			target: { value: "  Keep an eye on it.  " },
		});
		expect(mark().disabled).toBe(false);
		fireEvent.click(mark());
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: { command: "purchase_order_place", body: { order: 12 } },
		});
		await s.reply(sent, { said: "marked PO-12 placed", events: [98] });
		const filed = await waitFor(() => {
			const frames = s.calls("request.file");
			if (frames.length === 0) throw new Error("no request was filed");
			return frames[0];
		});
		expect((filed as { params: object }).params).toEqual({
			text: "Keep an eye on it.",
		});
	});

	it("mark_received_sends_what_was_paid_then_asks_to_update_the_register", async () => {
		const { container, s } = await opened("ivo");
		const open = async (order: string, seller: string) => {
			fireEvent.click(
				within(await itemOf(en.ordersPlacedTitle, order)).getByRole("button", {
					name: "Mark received",
				}),
			);
			return screen.findByRole("dialog", {
				name: `Mark PO-${order} from ${seller} received?`,
			});
		};
		const dialog = await open("11", "Northfield Mill");
		const before = asked(s);

		// What was paid is what was said when the order was placed; today is the day it came.
		const field = (name: string | RegExp) =>
			within(dialog).getByLabelText(name) as HTMLInputElement;
		expect(field("When did it come?").value).toBe("2026-10-26");
		expect(field("What did you pay?").value).toBe("190.00");
		expect(field("Currency").value).toBe("EUR");
		expect(field(/When does it renew/).value).toBe("");
		expect(
			within(dialog).getByText(
				"For a subscription, or anything you pay for again.",
			),
		).toBeTruthy();
		const request = () =>
			within(dialog).getByRole("textbox", {
				name: "Your request",
			}) as HTMLTextAreaElement;
		expect(
			within(dialog).getByRole("checkbox", {
				name: "Ask Ivo to update the register",
			}),
		).toBeTruthy();
		expect(request().value).toBe(
			"Update the register for PO-11 from Northfield Mill: it came on 26 October, and I paid 190.00 EUR.",
		);
		await expectNoAxeViolations(container);

		// A renewal day is said in the request, and goes with the order.
		fireEvent.change(field(/When does it renew/), {
			target: { value: "2027-10-26" },
		});
		expect(request().value).toBe(
			"Update the register for PO-11 from Northfield Mill: it came on 26 October, and I paid 190.00 EUR. It renews on 26 October 2027.",
		);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Mark received" }),
		);
		const marked = await sentCommand(s);
		expect(marked.params).toEqual({
			command: {
				command: "purchase_order_receive",
				body: {
					order: 11,
					received_on: "2026-10-26",
					paid: "190.00",
					currency: "EUR",
					renews_on: "2027-10-26",
				},
			},
		});
		await s.reply(marked, { said: "marked PO-11 received", events: [94] });
		const filed = await waitFor(() => {
			const frames = s.calls("request.file");
			if (frames.length === 0) throw new Error("no request was filed");
			return frames[0];
		});
		expect((filed as { params: object }).params).toEqual({
			text: "Update the register for PO-11 from Northfield Mill: it came on 26 October, and I paid 190.00 EUR. It renews on 26 October 2027.",
		});
		await s.reply(filed as never, { task_id: "FRK-51" });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		await waitFor(() => expect(asked(s)).toBe(before + 1));
	});

	it("mark_received_asks_what_was_paid_when_the_order_was_placed_without_it", async () => {
		const { s } = await opened("ivo");
		fireEvent.click(
			within(await itemOf(en.ordersPlacedTitle, "13")).getByRole("button", {
				name: "Mark received",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Mark PO-13 from Tin Town received?",
		});
		// The total stands in for what was paid, until the owner says otherwise.
		expect(
			(within(dialog).getByLabelText("What did you pay?") as HTMLInputElement)
				.value,
		).toBe("75.00");

		// With nothing paid, no amount or currency is sent, and the request says only that it came.
		fireEvent.change(within(dialog).getByLabelText("What did you pay?"), {
			target: { value: " " },
		});
		expect(
			(
				within(dialog).getByRole("textbox", {
					name: "Your request",
				}) as HTMLTextAreaElement
			).value,
		).toBe(
			"Update the register for PO-13 from Tin Town: it came on 26 October.",
		);
		fireEvent.click(
			within(dialog).getByRole("checkbox", {
				name: "Ask Ivo to update the register",
			}),
		);
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Mark received" }),
		);
		const marked = await sentCommand(s);
		expect(marked.params).toEqual({
			command: {
				command: "purchase_order_receive",
				body: { order: 13, received_on: "2026-10-26" },
			},
		});
		await s.reply(marked, { said: "marked PO-13 received", events: [95] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("request.file")).toHaveLength(0);
	});

	it("correcting_a_status_sends_it", async () => {
		const { container, s } = await opened("ivo");
		const open = async () => {
			fireEvent.click(
				within(await itemOf(en.ordersPlacedTitle, "10")).getByRole("button", {
					name: "Correct the status",
				}),
			);
			return screen.findByRole("dialog", {
				name: "Correct the status of PO-10?",
			});
		};
		const dialog = await open();
		await expectNoAxeViolations(container);
		const choice = (name: string) =>
			within(dialog).getByRole("radio", { name }) as HTMLInputElement;
		// The order's current status is chosen, and nothing is needed to keep it.
		expect(
			within(dialog)
				.getAllByRole("radio")
				.map((radio) => (radio as HTMLInputElement).labels?.[0]?.textContent),
		).toEqual(["Being prepared", "Shipped", "Delayed", "A problem"]);
		expect(choice("Shipped").checked).toBe(true);
		const save = () =>
			within(dialog).getByRole("button", { name: "Save the status" });
		expect((save() as HTMLButtonElement).disabled).toBe(false);
		expect(
			within(dialog).getByText(
				"Ivo reads your correction in its next piece of work.",
			),
		).toBeTruthy();

		// Nothing is needed for Shipped; Delayed needs what the owner knows and the day it is expected.
		expect(within(dialog).queryByText(/\(required\)/)).toBeNull();
		fireEvent.click(choice("Delayed"));
		expect(within(dialog).getByText("What you know (required)")).toBeTruthy();
		expect(within(dialog).getByText("Expected on (required)")).toBeTruthy();
		expect((save() as HTMLButtonElement).disabled).toBe(true);
		fireEvent.change(within(dialog).getByLabelText(/What you know/), {
			target: { value: "  The seller emailed: a week late.  " },
		});
		expect((save() as HTMLButtonElement).disabled).toBe(true);
		fireEvent.change(within(dialog).getByLabelText(/Expected on/), {
			target: { value: "2026-11-02" },
		});
		expect((save() as HTMLButtonElement).disabled).toBe(false);
		fireEvent.click(save());
		const delayed = await sentCommand(s);
		expect(delayed.params).toEqual({
			command: {
				command: "purchase_order_update",
				body: {
					order: 10,
					status: "delayed",
					note: "The seller emailed: a week late.",
					expected_on: "2026-11-02",
				},
			},
		});
		await s.reply(delayed, {
			error: {
				kind: "refused",
				detail: "purchase_order_status_invalid: delayed needs a day",
			},
		});
		expect((await within(dialog).findByRole("alert")).textContent).toBe(
			en.refusePurchaseOrderStatus,
		);

		// A problem needs what the owner knows, not a day.
		fireEvent.click(choice("A problem"));
		expect(within(dialog).getByText("What you know (required)")).toBeTruthy();
		expect(within(dialog).getByText("Expected on")).toBeTruthy();
		fireEvent.change(within(dialog).getByLabelText(/Expected on/), {
			target: { value: "" },
		});
		expect((save() as HTMLButtonElement).disabled).toBe(false);
		fireEvent.change(within(dialog).getByLabelText(/What you know/), {
			target: { value: "" },
		});
		expect((save() as HTMLButtonElement).disabled).toBe(true);
		fireEvent.change(within(dialog).getByLabelText(/What you know/), {
			target: { value: "Out of stock." },
		});
		fireEvent.click(save());
		const problem = await sentCommand(s, 2);
		expect(problem.params).toEqual({
			command: {
				command: "purchase_order_update",
				body: { order: 10, status: "problem", note: "Out of stock." },
			},
		});
		const before = asked(s);
		await s.reply(problem, { said: "corrected PO-10", events: [96] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		await waitFor(() => expect(asked(s)).toBe(before + 1));
	});

	it("a_status_that_needs_nothing_is_sent_as_chosen", async () => {
		const { s } = await opened("ivo");
		// No status yet: Being prepared is where it starts.
		fireEvent.click(
			within(await itemOf(en.ordersPlacedTitle, "13")).getByRole("button", {
				name: "Correct the status",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Correct the status of PO-13?",
		});
		expect(
			(
				within(dialog).getByRole("radio", {
					name: "Being prepared",
				}) as HTMLInputElement
			).checked,
		).toBe(true);
		fireEvent.click(within(dialog).getByRole("radio", { name: "Shipped" }));
		fireEvent.click(
			within(dialog).getByRole("button", { name: "Save the status" }),
		);
		const sent = await sentCommand(s);
		expect(sent.params).toEqual({
			command: {
				command: "purchase_order_update",
				body: { order: 13, status: "shipped" },
			},
		});
	});

	it("closing_asks_first", async () => {
		const { container, s } = await opened("ivo");
		const open = async () => {
			fireEvent.click(
				within(await itemOf(en.ordersPlacedTitle, "11")).getByRole("button", {
					name: "It won’t come",
				}),
			);
			return screen.findByRole("dialog", {
				name: "Close PO-11 without receiving it?",
			});
		};
		const dialog = await open();
		expect(
			within(dialog).getByText(
				"For an order the seller cancelled or refunded, or one that was lost. Ivo stops following it up.",
			),
		).toBeTruthy();
		await expectNoAxeViolations(container);

		// Keeping it sends nothing.
		fireEvent.click(within(dialog).getByRole("button", { name: "Keep it" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);

		// Closing it sends the order, and the owner's note when there is one.
		const again = await open();
		fireEvent.click(within(again).getByRole("button", { name: "Close it" }));
		const plain = await sentCommand(s);
		expect(plain.params).toEqual({
			command: { command: "purchase_order_close", body: { order: 11 } },
		});
		await s.reply(plain, {
			error: {
				kind: "refused",
				detail: "purchase_order_not_placed: order 11 is not placed",
			},
		});
		expect((await within(again).findByRole("alert")).textContent).toBe(
			en.refusePurchaseOrderNotPlaced,
		);
		fireEvent.change(within(again).getByLabelText(/A note for Ivo/), {
			target: { value: "  The seller refunded it.  " },
		});
		fireEvent.click(within(again).getByRole("button", { name: "Close it" }));
		const withNote = await sentCommand(s, 2);
		expect(withNote.params).toEqual({
			command: {
				command: "purchase_order_close",
				body: { order: 11, note: "The seller refunded it." },
			},
		});
		const before = asked(s);
		await s.reply(withNote, { said: "closed PO-11", events: [97] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		await waitFor(() => expect(asked(s)).toBe(before + 1));
	});

	it("ask_for_a_follow_up_files_the_owners_request", async () => {
		const { s } = await opened("ivo");
		fireEvent.click(
			within(await itemOf(en.ordersPlacedTitle, "11")).getByRole("button", {
				name: "Ask for a follow-up",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Ask Ivo to follow up PO-11?",
		});
		expect(
			(
				within(dialog).getByRole("textbox", {
					name: "Your request",
				}) as HTMLTextAreaElement
			).value,
		).toBe(
			"Follow up on PO-11 from Northfield Mill: where is it, and when will it come?",
		);
		expect(
			within(dialog).getByText(
				"Mira reads every request and asks you if anything is unclear.",
			),
		).toBeTruthy();
		fireEvent.click(
			within(dialog).getByRole("button", { name: en.requestSend }),
		);

		// Only the request is filed: the order is not touched.
		const filed = await waitFor(() => {
			const frames = s.calls("request.file");
			if (frames.length === 0) throw new Error("no request was filed");
			return frames[0];
		});
		expect((filed as { params: object }).params).toEqual({
			text: "Follow up on PO-11 from Northfield Mill: where is it, and when will it come?",
		});
		await s.reply(filed as never, { task_id: "FRK-52" });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);
	});

	it("writes_out_what_hides_text_in_the_agents_status_note", async () => {
		// U+202E reverses what follows it: the owner must read it written out, not obey it.
		const order = ORDERS.orders.find((one) => one.order === 11);
		await opened("ivo", {
			orders: [
				{
					...order,
					status: {
						status: "delayed",
						note: "Flour is short.\u202e",
						by: "agent",
						at: at(25, 10),
						expected_on: "2026-10-30",
					},
				},
			],
		});
		const delayed = await itemOf(en.ordersPlacedTitle, "11");
		const note = within(delayed).getByText(/Flour is short\./);
		expect(note.getAttribute("data-trust")).toBe("untrusted");
		expect(note.textContent).toContain("\\u{202e}");
		expect(note.textContent).not.toContain("\u202e");
	});

	it("writes_out_what_hides_text_in_the_seller_of_a_follow_up_request", async () => {
		const order = ORDERS.orders.find((one) => one.order === 11);
		await opened("ivo", {
			orders: [{ ...order, seller: "Northfield\u202e Mill" }],
		});
		fireEvent.click(
			within(await itemOf(en.ordersPlacedTitle, "11")).getByRole("button", {
				name: "Ask for a follow-up",
			}),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Ask Ivo to follow up PO-11?",
		});
		// The request is the owner's own words once sent: it says the seller as the owner reads it.
		const request = (
			within(dialog).getByRole("textbox", {
				name: "Your request",
			}) as HTMLTextAreaElement
		).value;
		expect(request).toBe(
			"Follow up on PO-11 from Northfield\\u{202e} Mill: where is it, and when will it come?",
		);
	});

	it("says_the_day_an_order_was_approved_not_the_day_it_was_drafted", async () => {
		const order = ORDERS.orders.find((one) => one.order === 12);
		// Drafted on 20 October and approved yesterday.
		await opened("ivo", {
			orders: [{ ...order, drafted_at: at(20, 9), decided_at: at(25, 9) }],
		});
		const toPlace = await itemOf(en.ordersToPlace, "12");
		expect(toPlace.textContent).toContain("Approved yesterday.");
		expect(toPlace.textContent).not.toContain("20 October");
	});

	it("a_received_order_with_no_price_says_none", async () => {
		const received = ORDERS.orders.find((one) => one.order === 4);
		await opened("ivo", {
			orders: [
				{ ...received, order: 21, paid: undefined, paid_currency: undefined },
			],
		});
		const recent = within(
			await screen.findByRole("list", { name: "Recent orders" }),
		).getAllByRole("listitem");
		expect(recent.map((one) => one.textContent)).toEqual([
			"PO-21Old MillReceived on 10 September.",
		]);
	});

	it("a_page_of_another_role_has_no_orders", async () => {
		const { s } = await opened("theo", null);
		expect(screen.queryByRole("heading", { name: "Orders" })).toBeNull();
		expect(asked(s)).toBe(0);
	});
});
