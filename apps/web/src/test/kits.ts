// Kit services as `team.get` answers them (ADR 0036), shared by the pages' tests.

/** What a role's kit offers Theo, as `team.get` answers it (ADR 0036). */
export const KIT_NOTION = {
	name: "notion",
	title: "Notion",
	about: "Notion is where your team keeps docs, notes and plans.",
	why: "Reads your product docs, so plans start from what you already wrote.",
	setup:
		"On Notion’s page, choose “New integration”, name it Catervas and pick your workspace. Copy the value labelled “Internal Integration Secret”.",
	key_page: "https://www.notion.so/profile/integrations",
	labels: {
		search: "search pages",
		read_page: "read a page",
		create_page: "create a page",
		delete_page: "delete a page",
	},
	auth: "keys",
	credential_keys: ["NOTION_KEY"],
};
export const KIT_LINEAR = {
	name: "linear",
	title: "Linear",
	about: "Linear is where your team keeps its backlog.",
	why: "Reads your backlog, so a request can start from an issue you already filed.",
	setup: "Sign in with your Linear account.",
	labels: {},
	auth: "oauth",
	credential_keys: [],
};
/** A kit service that signs in through Catervas Cloud, which no build can reach before the web launch. */
export const KIT_ADS = {
	name: "google-ads",
	title: "Google Ads",
	about:
		"Google Ads shows your ads to people searching on Google and charges you for the clicks.",
	why: "So the Marketing Specialist can find the words your customers search for and run the search ads in a marketing plan you approved, within its budget.",
	setup: "Sign in with the Google account that manages your ads.",
	labels: {},
	auth: "oauth",
	credential_keys: [],
	at_launch: true,
};
