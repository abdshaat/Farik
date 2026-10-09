// The Procurement Specialist's team and the sites it reads and asks about, as the daemon answers
// (spec 6.10, ADR 0039). Times are built in the browser's own time zone, so that the days the page
// says do not depend on where the tests run.

import { at, NOW } from "./posts.ts";

export { at, NOW };

const agent = (id: string, name: string, role: string, avatar: string) => ({
	id,
	display_name: name,
	role,
	avatar,
	persona: `${name} persona`,
	status: "active",
});

export const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager", "product-manager"),
		agent("theo", "Theo", "software_developer", "developer"),
		agent("ivo", "Ivo", "procurement_specialist", "extra-5"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};

/** What the page reads of the team's rules: every agent's effective model and tiers. */
export const EFFECTIVE = TEAM.agents.map(({ id }) => ({
	id,
	model: { id: "claude-sonnet-5-5", label: "Balanced model", effort: "medium" },
	tiers: ["read", "network"],
	base_tiers: ["read", "network"],
}));

/** Markup an agent can write into its reason: shown as typed, never made into an element. */
export const MARKUP = "<b>not bold</b>";

/** The row `waiting.list` gives while Ivo asks to read a site. */
export const SITE_ROW = {
	task_id: "FRK-31",
	kind: "site_request",
	agent_id: "ivo",
	title: "Find a supplier for 500 pie boxes",
	line: "Ivo asks to read pieboxpros.com",
	request: 61,
	host: "pieboxpros.com",
	url: "https://pieboxpros.com/printed-pie-boxes",
	why: `They print pie boxes with your logo from 250 boxes, and their price list is on their site. ${MARKUP}`,
};

/** A name that looks like uline.com, one letter of it Cyrillic, as Farik keeps it. */
export const SCRIPT_ROW = {
	...SITE_ROW,
	line: "Ivo asks to read xn--ulne-m9d.com",
	request: 62,
	host: "xn--ulne-m9d.com",
	url: "https://ulіne.com/pie-boxes",
	why: "A search result for bulk pie boxes points to this Uline page, with prices for 500 or more.",
};

const shop = (host: string, name: string, category: string) => ({
	host,
	shop: name,
	category,
	on: true,
});

/** `sites.list` as it answers for Ivo's page: Farik's sites by kind of shop, and the owner's own. */
export const SITES = {
	farik: [
		shop("amazon.com", "Amazon", "general_marketplace"),
		shop("ebay.com", "eBay", "general_marketplace"),
		shop("walmart.com", "Walmart", "general_marketplace"),
		shop("costco.com", "Costco", "general_marketplace"),
		shop("samsclub.com", "Sam’s Club", "general_marketplace"),
		shop("etsy.com", "Etsy", "general_marketplace"),
		shop("staples.com", "Staples", "office_supplies"),
		shop("officedepot.com", "Office Depot", "office_supplies"),
		shop("quill.com", "Quill", "office_supplies"),
		shop("grainger.com", "Grainger", "industrial_supplies"),
		shop("mcmaster.com", "McMaster-Carr", "industrial_supplies"),
		shop("fastenal.com", "Fastenal", "industrial_supplies"),
		shop("mscdirect.com", "MSC Industrial Supply", "industrial_supplies"),
		// Turned off once, then back on: the day is the last time the owner touched it.
		{
			...shop("uline.com", "Uline", "packaging_and_shipping"),
			at: new Date(2026, 9, 20, 9).toISOString(),
		},
		{
			...shop("papermart.com", "Paper Mart", "packaging_and_shipping"),
			on: false,
			at: new Date(2026, 9, 5, 9).toISOString(),
		},
		shop("ups.com", "UPS", "packaging_and_shipping"),
		shop("fedex.com", "FedEx", "packaging_and_shipping"),
		shop("store.usps.com", "USPS Postal Store", "packaging_and_shipping"),
		shop("cdw.com", "CDW", "computers_and_it"),
		shop("insight.com", "Insight", "computers_and_it"),
		shop("moo.com", "MOO", "printing"),
	],
	owner: [
		{
			host: "kitchenpartsdirect.com",
			at: new Date(2026, 8, 28, 9).toISOString(),
		},
		{
			host: "northfieldmill.com",
			at: new Date(2026, 9, 2, 9).toISOString(),
			request: 40,
		},
		{ host: "pieboxpros.com", at: at(26, 8), request: 61 },
	],
	waiting: [],
};
