import { t } from "../strings/t.ts";

/** What the hub pushes as `reminders.due`, in camelCase. */
export type Due = {
	at: string;
	count: number;
	headline: string;
	time: string;
};

/** Whether this browser lets Catervas show notices. */
export function noticesAllowed(): boolean {
	return (
		typeof Notification !== "undefined" && Notification.permission === "granted"
	);
}

/**
 * Shows the browser notice of a digest that is due, once: `seen` holds the digests already shown,
 * so a second push of the same one shows nothing. Clicking it brings this tab to Needs you today.
 */
export function showDue(due: Due, seen: Set<string>): void {
	if (!noticesAllowed() || seen.has(due.at)) return;
	seen.add(due.at);
	const body = t(
		due.count === 1 ? "remindersNoticeBodyOne" : "remindersNoticeBody",
		{
			count: due.count,
			headline: due.headline,
		},
	).trim();
	const notice = new Notification(
		t("remindersNoticeTitle", { time: due.time }),
		{
			body,
			tag: due.at,
		},
	);
	notice.onclick = () => {
		window.focus();
		history.pushState(null, "", "/needs-you");
		dispatchEvent(new PopStateEvent("popstate"));
		notice.close();
	};
}
