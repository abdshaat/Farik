import { expectNoAxeViolations } from "@farik/ui/test";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { en } from "../strings/en.ts";
import type { FakeSocket } from "../test/fake-socket.ts";
import { sentCommand } from "../test/gate.ts";
import { answerQuery, answerStatus, renderApp } from "../test/render-app.tsx";
import { at, EFFECTIVE, NOW, SITES, TEAM } from "../test/sites.ts";

/** One agent's page, its sites answered unless `sites` is left out. */
async function opened(agent: string, sites: object | null = SITES) {
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
	if (sites) await answerQuery(s, "sites.list", sites);
	await screen.findByRole("heading", {
		name:
			agent === "ivo"
				? "Ivo, your Procurement Specialist"
				: "Theo, your Developer",
	});
	return { container, s };
}

/** The body of a sent command. */
const bodyOf = (frame: ReturnType<FakeSocket["calls"]>[number]) =>
	(frame.params as { command: { command: string; body: object } }).command;
const sitesAsked = (s: FakeSocket) =>
	s.calls("query").filter((q) => q.params.name === "sites.list").length;
describe("the sites on the Procurement Specialist's page", () => {
	afterEach(() => {
		vi.useRealTimers();
		vi.unstubAllGlobals();
	});

	it("the_agent_page_lists_its_sites", async () => {
		const { container } = await opened("ivo");
		await expectNoAxeViolations(container);

		const heading = screen.getByRole("heading", { name: en.sitesTitle });
		const section = heading.closest("section") as HTMLElement;
		// It comes directly before what the agent may do.
		expect(section.nextElementSibling).toBe(
			screen
				.getByRole("heading", { name: "What Ivo may do" })
				.closest("section"),
		);
		expect(section.textContent).toContain(
			"Searches the whole web; opens pages only on these sites. Asks you on Today for others.",
		);
		expect(
			within(section).getByRole("heading", { name: en.sitesFarikTitle }),
		).toBeTruthy();

		// A closed category names its shops: all of three, and the first three of more.
		expect(
			within(section).getByRole("button", { name: /^Office supplies/ })
				.textContent,
		).toContain("Staples, Office Depot and Quill");
		expect(
			within(section).getByRole("button", { name: /^Industrial supplies/ })
				.textContent,
		).toContain("Grainger, McMaster-Carr, Fastenal and 1 more");
		expect(
			within(section).getByRole("button", { name: /^Marketplaces/ })
				.textContent,
		).toContain("Amazon, eBay, Walmart and 3 more");
		expect(
			within(section)
				.getByRole("button", { name: /^Marketplaces/ })
				.getAttribute("aria-expanded"),
		).toBe("false");

		// One with a shop turned off is open, and says so in its count; the shops show their switches.
		const packaging = within(section).getByRole("button", {
			name: /^Packaging and shipping/,
		});
		expect(packaging.getAttribute("aria-expanded")).toBe("true");
		expect(packaging.textContent).toContain("5 shops, 1 turned off");
		const uline = within(section).getByRole("switch", { name: "Uline" });
		expect(uline.getAttribute("aria-checked")).toBe("true");
		const mart = within(section).getByRole("switch", { name: "Paper Mart" });
		expect(mart.getAttribute("aria-checked")).toBe("false");
		expect(section.textContent).toContain("uline.com");
		expect(section.textContent).toContain("You turned it off on 5 October.");
		// Uline was turned off and on again: it is on, and says nothing of the day.
		expect(section.textContent?.match(/You turned it off/g)).toHaveLength(1);
		// A closed category's shops are not listed.
		expect(
			within(section).queryByRole("switch", { name: "Amazon" }),
		).toBeNull();
		fireEvent.click(
			within(section).getByRole("button", { name: /^Marketplaces/ }),
		);
		expect(
			within(section)
				.getByRole("switch", { name: "Amazon" })
				.getAttribute("aria-checked"),
		).toBe("true");
		expect(
			within(section).getByRole("button", { name: /^Marketplaces/ })
				.textContent,
		).toContain("6 shops");

		// The sites the owner allowed, with the day and how, each with its own Remove.
		const own = within(section).getByRole("list", { name: en.sitesOwnTitle });
		const rows = within(own).getAllByRole("listitem");
		expect(rows.map((row) => row.textContent)).toEqual([
			expect.stringContaining("kitchenpartsdirect.com"),
			expect.stringContaining("northfieldmill.com"),
			expect.stringContaining("pieboxpros.com"),
		]);
		expect(rows[0]?.textContent).toContain("Added by you on 28 September");
		expect(rows[1]?.textContent).toContain(
			"Allowed on 2 October, when Ivo asked",
		);
		expect(rows[2]?.textContent).toContain("Allowed today, when Ivo asked");
		expect(
			within(own).getByRole("button", { name: "Remove pieboxpros.com" }),
		).toBeTruthy();
		expect(within(section).getByLabelText(en.sitesAdd)).toBeTruthy();
	});

	it("days_are_said_as_today_yesterday_or_the_date", async () => {
		await opened("ivo", {
			...SITES,
			owner: [
				{ host: "a.example", at: at(26, 8), request: 1 },
				{ host: "b.example", at: at(25, 8) },
				{ host: "c.example", at: at(24, 8), request: 2 },
				{ host: "d.example", at: at(24, 8) },
			],
		});
		const own = screen.getByRole("list", { name: en.sitesOwnTitle });
		expect(
			within(own)
				.getAllByRole("listitem")
				.map((row) => row.textContent?.replace("Remove", "")),
		).toEqual([
			"a.exampleAllowed today, when Ivo asked",
			"b.exampleAdded by you yesterday",
			"c.exampleAllowed on 24 October, when Ivo asked",
			"d.exampleAdded by you on 24 October",
		]);
	});

	it("a_page_of_another_role_has_no_sites", async () => {
		const { s } = await opened("theo", null);
		expect(screen.queryByRole("heading", { name: en.sitesTitle })).toBeNull();
		expect(sitesAsked(s)).toBe(0);
	});

	it("a_switch_acts_at_once_and_asks_nothing", async () => {
		const { s } = await opened("ivo");
		fireEvent.click(screen.getByRole("switch", { name: "Uline" }));
		const off = await sentCommand(s);
		expect(bodyOf(off)).toEqual({
			command: "site_remove",
			body: { host: "uline.com" },
		});
		expect(screen.queryByRole("dialog")).toBeNull();
		await s.reply(off, { said: "Removed uline.com", events: [80] });
		// The list is read again once it is done.
		await waitFor(() => expect(sitesAsked(s)).toBe(2));

		fireEvent.click(screen.getByRole("switch", { name: "Paper Mart" }));
		const on = await sentCommand(s, 2);
		expect(bodyOf(on)).toEqual({
			command: "site_add",
			body: { site: "papermart.com" },
		});
		expect(screen.queryByRole("dialog")).toBeNull();
		// A refusal is said in words.
		await s.reply(on, {
			error: {
				kind: "refused",
				detail: "site_already_allowed: papermart.com is allowed already",
			},
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseSiteAlreadyAllowed,
		);
	});

	it("removing_a_site_asks_first", async () => {
		const { container, s } = await opened("ivo");
		fireEvent.click(
			screen.getByRole("button", { name: "Remove pieboxpros.com" }),
		);
		const dialog = await screen.findByRole("dialog", {
			name: "Remove pieboxpros.com?",
		});
		expect(dialog.textContent).toContain(
			"Ivo will no longer read pieboxpros.com.",
		);
		expect(dialog.textContent).toContain(
			"To let Ivo read it again, add it again here, or allow it when Ivo asks.",
		);
		await expectNoAxeViolations(container);
		expect(s.calls("command")).toHaveLength(0);

		fireEvent.click(within(dialog).getByRole("button", { name: "Keep it" }));
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
		expect(s.calls("command")).toHaveLength(0);

		fireEvent.click(
			screen.getByRole("button", { name: "Remove pieboxpros.com" }),
		);
		const again = await screen.findByRole("dialog", {
			name: "Remove pieboxpros.com?",
		});
		fireEvent.click(within(again).getByRole("button", { name: "Remove" }));
		const refused = await sentCommand(s);
		expect(bodyOf(refused)).toEqual({
			command: "site_remove",
			body: { host: "pieboxpros.com" },
		});
		// A refusal is said in the dialog, which stays.
		await s.reply(refused, {
			error: {
				kind: "refused",
				detail: "site_not_allowed: pieboxpros.com is not allowed now",
			},
		});
		expect((await within(again).findByRole("alert")).textContent).toBe(
			en.refuseSiteNotAllowed,
		);
		fireEvent.click(within(again).getByRole("button", { name: "Remove" }));
		const removed = await sentCommand(s, 2);
		await s.reply(removed, { said: "Removed pieboxpros.com", events: [81] });
		await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
	});

	it("adding_a_site_keeps_its_name", async () => {
		const { s } = await opened("ivo");
		const field = screen.getByLabelText(en.sitesAdd);
		fireEvent.change(field, {
			target: { value: "  https://www.pietinsupply.com/pie-tins/9-inch " },
		});
		fireEvent.click(screen.getByRole("button", { name: en.sitesAddButton }));
		const added = await sentCommand(s);
		// The words go as they were written, trimmed: the daemon keeps the site.
		expect(bodyOf(added)).toEqual({
			command: "site_add",
			body: { site: "https://www.pietinsupply.com/pie-tins/9-inch" },
		});
		await s.reply(added, { said: "Allowed pietinsupply.com.", events: [82] });
		expect(
			await screen.findByText("Ivo may now read pietinsupply.com."),
		).toBeTruthy();
		expect((field as HTMLInputElement).value).toBe("");

		// Words that name no site are refused in words, and stay in the field.
		fireEvent.change(field, { target: { value: "not a site" } });
		fireEvent.click(screen.getByRole("button", { name: en.sitesAddButton }));
		const bad = await sentCommand(s, 2);
		expect(bodyOf(bad).body).toEqual({ site: "not a site" });
		await s.reply(bad, {
			error: {
				kind: "refused",
				detail: 'site_invalid: "not a site" it is not an address',
			},
		});
		expect((await screen.findByRole("alert")).textContent).toBe(
			en.refuseSiteInvalid,
		);
		expect((field as HTMLInputElement).value).toBe("not a site");
	});
});
