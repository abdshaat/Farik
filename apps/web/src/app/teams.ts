import { landing } from "./landing.ts";
import type { ServeStatus } from "./store.ts";

/** Where the last team viewed is kept, so that an old address opens that team's page. */
export const LAST_TEAM = "catervas.lastTeam";

/** The router state that sends "/" to the team Catervas just took on, not the one remembered. */
export const ON_CURRENT = { team: "current" } as const;

/** A team as `teams.list` shows it, in camelCase. */
export type TeamRow = {
	slug: string;
	name: string;
	root: string;
	/** `running` and `paused` are driven here; the others are not. */
	status: "running" | "paused" | "elsewhere" | "missing" | "failed";
	workingAgents: number;
	waiting: number;
	todayUsd: number;
	monthUsd: number;
	failed?: string;
	/** A team Catervas made from the proposal that has not started a session: Undo takes it back. */
	undoable?: boolean;
};

/** The `teams.list` answer. */
export type TeamList = {
	teams: TeamRow[];
	current: string | null;
	accountAsleepUntil: string | null;
};

/** The team viewed last, if this browser kept one. */
export function lastTeam(): string | null {
	try {
		return localStorage.getItem(LAST_TEAM);
	} catch {
		return null;
	}
}

/** Keeps `slug` as the team viewed last. */
export function rememberTeam(slug: string) {
	try {
		localStorage.setItem(LAST_TEAM, slug);
	} catch {}
}

/** Forgets the team viewed last. */
export function forgetTeam() {
	try {
		localStorage.removeItem(LAST_TEAM);
	} catch {}
}

/** The addresses that belong to no team: setup, All teams, This computer, a team's own. */
const NOT_A_TEAMS =
	/^\/(setup|teams|computer|needs-you|find|connect|t)([/?#]|$)/;

/** Whether `path` is a page of a team, which `/t/<team>` goes before. */
export function isTeamAddress(path: string): boolean {
	return path.startsWith("/") && !NOT_A_TEAMS.test(path);
}

/** `path`, a page of a team, in the address of `team`. */
export function teamPath(team: string, path: string): string {
	if (!isTeamAddress(path)) return path;
	return `/t/${team}${path === "/" ? "/" : path}`;
}

/** The team a pathname names, if it is under `/t/`. */
export function teamIn(pathname: string): string | undefined {
	return /^\/t\/([^/]+)/.exec(pathname)?.[1];
}

/** `pathname` without the `/t/<team>` before it: the page, as the team's routes name it. */
export function pageOf(pathname: string): string {
	const rest = pathname.replace(/^\/t\/[^/]+/, "");
	return rest === "" ? "/" : rest;
}

/**
 * Where an address with no `/t/` goes: the same page of the last team viewed when it is still a
 * team, else of the only team; with several and none remembered, All teams; with none, setup.
 * `onCurrent` prefers the team Catervas just took on.
 */
export function redirectTarget(
	address: string,
	list: TeamList,
	status: ServeStatus,
	onCurrent: boolean,
): string {
	const { teams, current } = list;
	if (teams.length === 0) {
		const there = landing(status);
		return there === "/" ? "/setup/computer" : there;
	}
	const has = (slug: string | null): slug is string =>
		slug !== null && teams.some((team) => team.slug === slug);
	const last = lastTeam();
	const first = teams[0];
	const pick =
		onCurrent && has(current)
			? current
			: has(last)
				? last
				: teams.length === 1 && first
					? first.slug
					: null;
	return pick === null ? "/teams" : teamPath(pick, address);
}

/**
 * The page of another team that matches `page`: the same page, except that an id of the old team
 * (a task, a request, a file, an agent, a chat) means nothing in the new one.
 */
export function samePage(page: string): string {
	if (/^\/(tasks|requests|files|marketing)\//.test(page)) return "/";
	if (/^\/team\/[^/]+/.test(page)) return "/team";
	if (/^\/channel\/[^/]+/.test(page)) return "/channel";
	if (/^\/(teams|computer|needs-you)/.test(page)) return "/";
	return page;
}
