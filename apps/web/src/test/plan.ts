// The team and the plan awaiting approval that the plan pages' tests share.

const agent = (id: string, name: string, role: string) => ({
	id,
	display_name: name,
	role,
	avatar: role,
	status: "active",
});
export const TEAM = {
	name: "Corner Bakery",
	agents: [
		agent("mira", "Mira", "product_manager"),
		agent("ada", "Ada", "architect"),
		agent("theo", "Theo", "software_developer"),
	],
	budgets: {},
	policy: { integration: "auto_merge" },
	rules: {},
};
export const SUMMARY =
	"Customers will be able to buy a gift card, send it to a friend by email, and use it when they order.";
/** A plan awaiting approval, as `contract.get` answers it. */
export const CONTRACT = {
	id: "CTV-1",
	title: "Gift cards",
	kind: "epic",
	intent: "Customers can give a gift card to a friend and use it to pay.",
	summary: SUMMARY,
	scope: {
		in_scope: ["Gift cards bought online"],
		out_of_scope: ["Printed gift cards", "Gift cards at the café counter"],
	},
	requirements: [
		{
			id: "R1",
			text: "A customer picks $25, $50 or $100, pays, and gets a receipt.",
		},
		{
			id: "R2",
			text: "The card arrives by email with a code and a short message.",
		},
	],
	exit_criteria: [
		{
			id: "C1",
			text: "A test purchase of each amount goes through.",
			satisfies: ["R1"],
			verification: { method: "test", command: "pnpm test gift-cards" },
		},
		{
			id: "C2",
			text: "A test email arrives with a working code.",
			satisfies: ["R2"],
			verification: {
				method: "review",
				rubric: ["A test email arrives with a working code."],
			},
		},
	],
	assignee_role: "software_developer",
	reviewer_role: "architect",
	risk: "medium",
	budget: { max_cost_usd: 14 },
	allowed_paths: ["src/**"],
	status: "escalated",
	locked: false,
	created_by: "human",
	created_at: "2026-09-24T09:14:00Z",
	updated_at: "2026-09-24T10:02:00Z",
};
