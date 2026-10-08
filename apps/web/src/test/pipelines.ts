// The data sources Ivo asks for and the Product Manager passes to the owner, as `waiting.list`
// gives them (spec 6.10, ADR 0039). The words are the approved mockups'.

import { MARKUP, SITE_ROW } from "./sites.ts";

export { MARKUP, TEAM } from "./sites.ts";

/** A right-to-left override, which would reorder what follows it; the page writes it out. */
export const HIDDEN = "\u202e";

const TASK = {
	task_id: SITE_ROW.task_id,
	agent_id: "ivo",
	title: SITE_ROW.title,
};

/** A source that costs money, passed on with the Product Manager's reason. */
export const PAID_ROW = {
	...TASK,
	kind: "data_pipeline",
	line: "Ivo asks for a data source: Firecrawl",
	pipeline: 7,
	name: "Firecrawl",
	what: `Reads a seller’s page as text, including pages that show their prices only in a full browser.${HIDDEN} ${MARKUP}`,
	url: "https://www.firecrawl.dev/pricing",
	host: "firecrawl.dev",
	why: "Two of the five box sellers show their prices only in a full browser, so I cannot read them.",
	cost: "paid",
	needs_account: true,
	sends_project_data: false,
	reason: `It needs a paid plan, so it is your call.${HIDDEN} ${MARKUP}`,
	at: "2026-10-26T08:00:00Z",
	request_text: `Set up Firecrawl for the Procurement Specialist.\nWhat it gives: Reads a seller’s page as text.\nSource: https://www.firecrawl.dev/pricing\nAsked because: Two of the five box sellers show their prices only in a full browser.${HIDDEN}`,
};

/** A free source that would get the bakery's address, with no account needed. */
export const DATA_ROW = {
	...PAID_ROW,
	line: "Ivo asks for a data source: Shippo",
	pipeline: 8,
	name: "Shippo",
	url: "https://goshippo.com/shipping-api",
	host: "goshippo.com",
	cost: "free",
	needs_account: false,
	sends_project_data: true,
	reason: "Getting rates is free, but Shippo would get the bakery’s address.",
	request_text: "Set up Shippo for the Procurement Specialist.",
};

/** A source whose cost Ivo could not find, which the Product Manager did not decide in three tries. */
export const UNDECIDED_ROW = {
	...PAID_ROW,
	line: "Ivo asks for a data source: Azure prices",
	pipeline: 9,
	name: "Azure prices",
	cost: "unknown",
	reason: undefined,
	host: "prices.azure.com",
	url: "https://prices.azure.com/api/retail/prices",
};

/** A name written in another alphabet, kept in its `xn--` form. */
export const SCRIPT_PIPELINE_ROW = {
	...PAID_ROW,
	line: "Ivo asks for a data source: Ulіne",
	pipeline: 10,
	name: "Ulіne",
	host: "xn--ulne-m9d.com",
	url: "https://ulіne.com/prices",
};

/** An address that is not a web page: shown as text, with nothing to open. */
export const ODD_ADDRESS_ROW = {
	...PAID_ROW,
	line: "Ivo asks for a data source: Odd",
	pipeline: 11,
	name: "Odd",
	url: "javascript:alert(1)",
};
