// The Procurement Specialist's team, as the daemon answers (spec 6.10, ADR 0039). Times are built in the browser's own time zone, so that the days the page
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
