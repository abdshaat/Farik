/** A forgotten session's id. */
export const SESSION_ID = "0b1f6a54-2c1e-4d7e-9a0a-5b7f3c2d9e11";

const give = (text: string, team = "tidepool", teamName = "Tidepool") => ({
	text,
	team,
	team_name: teamName,
});

/** The `reminders.get` answer of the mockup: nine things, and two of today's runs. */
export const DIGEST = {
	at: "2026-10-22T09:00:00Z",
	count: 9,
	headline: "A price check failed, and Tidepool's last deploy failed.",
	time: "09:00",
	browser: true,
	system: true,
	notified: null,
	groups: [
		{
			group: "forgotten",
			items: [
				{
					key: `forgotten:${SESSION_ID}`,
					kind: "session",
					title: "Add CSV export · recipe-cards",
					detail: "4 days ago · 3 to-dos still open",
					project: "recipe-cards",
					resume: {
						command: `cd ~/code/recipe-cards && claude --resume ${SESSION_ID}`,
					},
					give: give(
						"Finish “Add CSV export” in recipe-cards. The work was left 4 days ago with 3 to-dos still open, on the branch main.",
					),
				},
				{
					key: "forgotten:s-2",
					kind: "session",
					title: "Fix the login redirect · tidepool-web",
					detail: "2 days ago · changes not pushed",
					project: "tidepool-web",
					resume: {
						command: "cd ~/code/tidepool-web && claude --resume s-2",
					},
					give: give(
						"Finish “Fix the login redirect” in tidepool-web. The work was left 2 days ago with changes not pushed, on the branch fix/login-redirect.",
					),
				},
			],
		},
		{
			group: "ending",
			items: [
				{
					key: "ending:s-3:t-1",
					kind: "cron",
					explain: "cron",
					title: "“Session cron 0 7 * * *” ends tomorrow",
					detail: "price-watch",
					project: "price-watch",
					resume: {
						command: "cd ~/code/price-watch && claude --resume s-3",
					},
				},
			],
		},
		{
			group: "schedules",
			items: [
				{
					key: "schedules:failed:octo/price-watch:price.yml",
					kind: "schedule",
					title: "Hourly price check failed",
					detail: "price-watch · 07:00 today",
					project: "price-watch",
					github_url: "https://github.com/octo/price-watch/actions",
					give: give(
						"Look into this in price-watch: Hourly price check failed. price-watch · 07:00 today.",
						"price-watch",
						"Price Watch",
					),
				},
				{
					key: "schedules:off:octo/infra:backup.yml",
					kind: "schedule",
					explain: "off",
					title: "GitHub turned off Weekly database backup",
					detail: "tidepool-infra",
					project: "tidepool-infra",
					github_url: "https://github.com/octo/infra/actions",
				},
			],
		},
		{
			group: "teams",
			items: [
				{
					key: "teams:waiting:tidepool",
					kind: "team",
					title: "Tidepool: Mira has waited 2 days for your answer",
					detail: "Which payment plans to offer",
					answer: { team: "tidepool" },
				},
			],
		},
		{
			group: "services",
			items: [
				{
					key: "services:zoho-books",
					kind: "service",
					title: "Zoho Books needs you to sign in again in Claude",
					detail: "Bookkeeping",
				},
			],
		},
		{
			group: "runs",
			items: [
				{
					key: "runs:octo/price-watch:ok.yml",
					kind: "run",
					title: "Nightly backup ran",
					detail: "price-watch · 03:00 today",
				},
				{
					key: "runs:octo/price-watch:report.yml",
					kind: "run",
					title: "Weekly report ran",
					detail: "price-watch · 06:00 today",
				},
			],
		},
	],
};
