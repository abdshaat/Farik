import { useCallback, useEffect, useState } from "react";
import { useConnection, useHub } from "../app/connection.tsx";

/** What the request of an item that can be given to its team says, and whose team it is. */
export type Give = {
	text: string;
	team?: string;
	teamName?: string;
};

/** One item of the daily digest, as `reminders.get` and `reminders.mine` answer it, in camelCase. */
export type ReminderItemData = {
	key: string;
	kind:
		| "session"
		| "cron"
		| "schedule"
		| "deploy"
		| "team"
		| "service"
		| "run"
		| "connect_github";
	title: string;
	detail: string;
	project?: string;
	/** What the item is, said behind an info button. */
	explain?: "cron" | "off";
	resume?: { command: string };
	claudeAiUrl?: string;
	githubUrl?: string;
	give?: Give;
	answer?: { team: string };
	connectGithub?: boolean;
};

export type GroupName =
	| "forgotten"
	| "ending"
	| "schedules"
	| "teams"
	| "services"
	| "runs";

/** The `reminders.get` answer. */
export type Digest = {
	at: string;
	count: number;
	headline: string;
	time: string;
	browser: boolean;
	system: boolean;
	notified: "browser" | "system" | "none" | null;
	groups: { group: GroupName; items: ReminderItemData[] }[];
};

/** The `reminders.mine` answer. */
export type Mine = { at: string; count: number; items: ReminderItemData[] };

/** The computer's digest, asked of the hub on mount and again on `again()`. */
export function useDigest(): { digest: Digest | undefined; again: () => void } {
	const { client } = useHub();
	const [digest, setDigest] = useState<Digest>();
	const [round, setRound] = useState(0);
	// biome-ignore lint/correctness/useExhaustiveDependencies: round asks for the digest again
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.call("reminders.get", {}).then(
			(answer) => live && setDigest(answer as Digest),
			() => {},
		);
		return () => {
			live = false;
		};
	}, [client, round]);
	const again = useCallback(() => setRound((n) => n + 1), []);
	return { digest, again };
}

/** The part of the digest that is the current team's, asked of that team's daemon. */
export function useMine(): { mine: Mine | undefined; again: () => void } {
	const { client } = useConnection();
	const [mine, setMine] = useState<Mine>();
	const [round, setRound] = useState(0);
	// biome-ignore lint/correctness/useExhaustiveDependencies: round asks for the part again
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.call("reminders.mine", {}).then(
			(answer) => live && setMine(answer as Mine),
			() => {},
		);
		return () => {
			live = false;
		};
	}, [client, round]);
	const again = useCallback(() => setRound((n) => n + 1), []);
	return { mine, again };
}
