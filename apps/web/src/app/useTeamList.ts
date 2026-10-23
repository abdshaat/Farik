import { useEffect } from "react";
import { useHubQuery } from "./store.ts";
import type { TeamList } from "./teams.ts";

const EVERY_MS = 10_000;

/** The hub's `teams.list`, asked again after any hub event and every 10 seconds. */
export function useTeamList(): {
	list: TeamList | undefined;
	again: () => void;
} {
	const { data, again } = useHubQuery<TeamList>("teams.list", {});
	useEffect(() => {
		const timer = setInterval(again, EVERY_MS);
		return () => clearInterval(timer);
	}, [again]);
	return { list: data, again };
}
