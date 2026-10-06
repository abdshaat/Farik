// What the plan's page says of the posts written for its slots (ADR 0042): where each slot stands,
// the counts of them, and what ending the plan does to them. Times are in the browser's time zone.

import { t } from "../strings/t.ts";
import { clock, dayWords, shortDate } from "./PostGoingOut.tsx";

/** A post written for one of a plan's slots, as `marketing_plan.get` lists it, in camelCase. */
export type WrittenPost = {
	post: number;
	slot: string;
	text: string;
	at: string;
	state: "scheduled" | "sent" | "stopped" | "missed" | "failed";
	stateAt: string;
	stoppedBy?: "owner" | "declined" | "plan_ended";
	missedWhy?: "not_running" | "paused" | "undecided";
};

/** Where a slot stands, by its newest post that did not fail. */
type Standing =
	| "none"
	| "going_out"
	| "sent"
	| "stopped_by_you"
	| "stopped_with_plan"
	| "missed";

/**
 * A slot's newest post, how it stands, and when a post of it failed. A slot whose newest post
 * failed has none. The posts are as the daemon lists them, oldest first.
 */
export function slotStanding(
	written: WrittenPost[],
	key: string,
): { shown: WrittenPost | undefined; standing: Standing; failedAt?: string } {
	const mine = written.filter((post) => post.slot === key);
	const newest = mine.at(-1);
	const shown = newest?.state === "failed" ? undefined : newest;
	const failed = mine.filter((post) => post.state === "failed").at(-1);
	const standing: Standing = !shown
		? "none"
		: shown.state === "scheduled"
			? "going_out"
			: shown.state === "sent"
				? "sent"
				: shown.state === "missed"
					? "missed"
					: shown.stoppedBy === "plan_ended"
						? "stopped_with_plan"
						: "stopped_by_you";
	return { shown, standing, ...(failed ? { failedAt: failed.stateAt } : {}) };
}

/** The words after a slot's state: "today at 18:00", "at 10:00", "on Tue 13 Oct", "when the plan ended". */
export function standingDetail(
	standing: Standing,
	post: WrittenPost | undefined,
	now: Date,
): string {
	if (!post) return "";
	const at = new Date(post.at);
	switch (standing) {
		case "going_out":
			return dayWords(at, now) === t("postToday")
				? `${t("postToday")} ${t("planSlotAt", { time: clock(at) })}`
				: t("planSlotAt", { time: clock(at) });
		case "sent":
			return t("planSlotAt", { time: clock(at) });
		case "stopped_by_you":
			return t("planSlotOn", { day: shortDate(new Date(post.stateAt)) });
		case "stopped_with_plan":
			return t("planSlotWhenEnded");
		case "missed":
			return t(
				post.missedWhy === "paused" ? "planSlotPaused" : "planSlotNotRunning",
			);
		default:
			return "";
	}
}

/** The slot's state in a word. */
export function standingWord(standing: Standing): string {
	switch (standing) {
		case "going_out":
			return t("planSlotGoingOut");
		case "sent":
			return t("planSlotSent");
		case "stopped_by_you":
			return t("planSlotStoppedByYou");
		case "stopped_with_plan":
			return t("planSlotStopped");
		case "missed":
			return t("planSlotMissed");
		default:
			return t("planSlotNone");
	}
}

/**
 * "1 sent, 1 going out, 1 stopped by you, 2 missed, and 1 not written yet.": the slots by how they
 * stand, a count of none left out, and a slot stopped with its plan counted nowhere. Nothing when
 * no clause is left.
 */
export function standingCounts(
	written: WrittenPost[],
	slots: string[],
): string {
	const counted = slots.map((key) => slotStanding(written, key).standing);
	const n = (standing: Standing) =>
		counted.filter((one) => one === standing).length;
	const clauses = (
		[
			["planPostsSent", n("sent")],
			["planPostsGoingOut", n("going_out")],
			["planPostsStopped", n("stopped_by_you")],
			["planPostsMissed", n("missed")],
			["planPostsNotWritten", n("none")],
		] as const
	)
		.filter(([, count]) => count > 0)
		.map(([key, count]) => t(key, { n: count }));
	if (clauses.length === 0) return "";
	if (clauses.length === 1) return `${clauses[0]}.`;
	if (clauses.length === 2) return `${clauses[0]} and ${clauses[1]}.`;
	return `${clauses.slice(0, -1).join(", ")}, and ${clauses.at(-1)}.`;
}

/**
 * What ending the plan does to its posts, in the end dialog's words: the ones not yet sent will not
 * go out, and the ones already with Buffer stay until they are stopped on Today. A line of none is
 * left out.
 */
export function endingPosts(written: WrittenPost[], now: Date): string[] {
	const notSent = written.filter((post) => post.state === "scheduled").length;
	const withBuffer = written
		.filter((post) => post.state === "sent" && new Date(post.at) > now)
		.sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
	const first = withBuffer[0];
	const lines: string[] = [];
	if (notSent > 0)
		lines.push(
			notSent === 1
				? t("marketingEndNotSentOne")
				: t("marketingEndNotSent", { n: notSent }),
		);
	if (first) {
		const at = new Date(first.at);
		const fill = { day: dayWords(at, now), time: clock(at) };
		lines.push(
			withBuffer.length === 1
				? t("marketingEndWithBufferOne", fill)
				: t("marketingEndWithBuffer", { ...fill, n: withBuffer.length }),
		);
	}
	return lines;
}
