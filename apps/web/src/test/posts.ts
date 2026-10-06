// The posts Today shows and the plan's page counts, as the daemon answers (ADR 0042). Times are
// built in the browser's own time zone, so that the words the page says do not depend on where the
// tests run.

import { vi } from "vitest";
import type { FakeSocket } from "./fake-socket.ts";
import { TEAM } from "./marketing.ts";
import { answerQuery, answerStatus, renderApp } from "./render-app.tsx";

/** Monday 26 October 2026, 09:15 in the browser's time zone. */
export const NOW = new Date(2026, 9, 26, 9, 15);

/** A time of October 2026 in the browser's time zone, as the daemon words it (UTC). */
export const at = (day: number, hour: number, minute = 0) =>
	new Date(2026, 9, day, hour, minute).toISOString();

const PIES = "https://cdn.example.com/pies.png";
const CLIP = "https://cdn.example.com/cookies.mp4";

/** Markup an agent or Buffer can write: shown as typed, never made into an element. */
export const MARKUP = "<img src=x onerror=alert(1)>";

/** What `social_posts.list` answers: four going out, soonest first, and two that did not. */
export const GOING_OUT = {
	posts: [
		{
			post: 41,
			agent_id: "kai",
			channel: "x",
			text: "We open at 10 today, not 7: the oven is being serviced this morning.",
			media: [],
			at: at(26, 10),
			hands_over_at: at(26, 9),
			state: "sent",
			approved_by: "owner",
		},
		{
			post: 42,
			agent_id: "kai",
			channel: "instagram",
			text: "Thanksgiving pies are open for pre-order.",
			media: [{ url: PIES, kind: "image" }],
			at: at(26, 13),
			hands_over_at: at(26, 12),
			state: "scheduled",
			plan: "MP-3",
			slot: "p6",
			approved_by: "plan",
		},
		{
			post: 43,
			agent_id: "kai",
			channel: "x",
			text: "Pie pre-orders are open: pumpkin, pecan and apple.",
			media: [],
			at: at(28, 8, 30),
			hands_over_at: at(28, 7, 30),
			state: "scheduled",
			plan: "MP-3",
			slot: "p7",
			approved_by: "plan",
		},
		{
			post: 44,
			agent_id: "kai",
			channel: "instagram",
			text: "Halloween sugar cookies, today only.",
			media: [{ url: CLIP, kind: "video" }],
			at: at(31, 9),
			hands_over_at: at(31, 8),
			state: "scheduled",
			plan: "MP-3",
			slot: "p8",
			approved_by: "plan",
		},
		{
			post: 40,
			agent_id: "kai",
			channel: "instagram",
			text: `Thanksgiving pies ${MARKUP}`,
			media: [],
			at: at(26, 8),
			hands_over_at: at(26, 7),
			state: "failed",
			plan: "MP-3",
			slot: "p5",
			approved_by: "plan",
			reason: `Buffer did not take it: “The image is too small ${MARKUP}”`,
		},
		{
			post: 39,
			agent_id: "kai",
			channel: "x",
			text: "A rainy week ahead: soup and a fresh roll.",
			media: [],
			at: at(25, 18),
			hands_over_at: at(25, 17),
			state: "missed",
			plan: "MP-3",
			slot: "p4",
			approved_by: "plan",
			missed_why: "not_running",
		},
		{
			post: 38,
			agent_id: "kai",
			channel: "x",
			text: "Half the pie pre-orders are taken.",
			media: [],
			at: at(25, 12),
			hands_over_at: at(25, 11),
			state: "missed",
			plan: "MP-3",
			slot: "p3",
			approved_by: "plan",
			missed_why: "paused",
		},
		{
			post: 37,
			agent_id: "kai",
			channel: "threads",
			text: "Come and see the new ovens.",
			media: [],
			at: at(25, 10),
			hands_over_at: at(25, 9),
			state: "missed",
			approved_by: "owner",
			missed_why: "undecided",
		},
	],
};

/** The row `waiting.list` gives while Kai's post outside the plan waits on the owner. */
export const POST_ROW = {
	task_id: "FRK-31",
	kind: "social_post",
	agent_id: "kai",
	title: "Autumn marketing plan",
	line: "Kai wants to post on Instagram",
	post: 45,
	channel: "instagram",
	text: `Bake with us: a sourdough evening at the bakery on Thursday 5 November. ${MARKUP}`,
	media: [{ url: PIES, kind: "image" }],
	at: at(29, 18),
};

/**
 * Today at `NOW`, with the team and each list answered: the posts, and what waits on the owner.
 * The caller undoes the clock with `vi.useRealTimers`.
 */
export async function todayWith(fields: {
	posts?: unknown;
	waiting?: unknown[];
}) {
	vi.useFakeTimers({ toFake: ["Date"] });
	vi.setSystemTime(NOW);
	const { container, socket } = await renderApp("/");
	const s = socket as FakeSocket;
	await answerStatus(s, false);
	await answerQuery(s, "team.get", { team: TEAM });
	await answerQuery(s, "team.activity", { activity: [] });
	await answerQuery(s, "waiting.list", { waiting: fields.waiting ?? [] });
	await answerQuery(s, "moved.since", { moved: [] });
	await answerQuery(s, "sprint.current", null);
	await answerQuery(s, "social_posts.list", fields.posts ?? { posts: [] });
	return { container, s };
}
