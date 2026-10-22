import { useEffect, useState } from "react";
import { useHub } from "../../app/connection.tsx";

/** How often the page asks `finding.status` again while a scan runs. */
const POLL_MS = 1000;

/** `finding.status`'s answer, in camelCase. */
export type FindingStatus = {
	phase: "idle" | "running" | "stopped" | "done";
	filesTotal: number;
	filesRead: number;
	sessions: number;
	projects: number;
	jobs: number;
	reading: string | null;
	unreadable: number;
	error: string | null;
	startedAt: string | null;
	endedAt: string | null;
	secondsLeft: number | null;
	paused: boolean;
	indexExists: boolean;
	exportDismissed: boolean;
	/** Counts up each time the hub publishes the scan's counts. */
	version: number;
};

/**
 * Where the scan is, asked once on mount and again every second while it runs. `again` asks now.
 */
export function useFindingStatus(): {
	status: FindingStatus | undefined;
	again: () => void;
} {
	const { client } = useHub();
	const [status, setStatus] = useState<FindingStatus>();
	const [round, setRound] = useState(0);
	// biome-ignore lint/correctness/useExhaustiveDependencies: round asks for the status again
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.call("finding.status", {}).then(
			(answer) => live && setStatus(answer as FindingStatus),
			() => {},
		);
		return () => {
			live = false;
		};
	}, [client, round]);
	const running = status?.phase === "running";
	// biome-ignore lint/correctness/useExhaustiveDependencies: each answer is a new `status`, and the next ask is timed from it
	useEffect(() => {
		if (!running) return;
		const timer = setTimeout(() => setRound((n) => n + 1), POLL_MS);
		return () => clearTimeout(timer);
	}, [status, running]);
	return { status, again: () => setRound((n) => n + 1) };
}

/** What `finding.offer` says: whether Claude Code history is on this computer; none until it answers. */
export function useOffer(): "claude_code" | "none" | undefined {
	const { client } = useHub();
	const [history, setHistory] = useState<"claude_code" | "none">();
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.call("finding.offer", {}).then(
			(answer) =>
				live &&
				setHistory((answer as { history: "claude_code" | "none" }).history),
			() => {},
		);
		return () => {
			live = false;
		};
	}, [client]);
	return history;
}

/** A value kept in this browser; none when storage refuses (a private window). */
export function kept(key: string): string | undefined {
	try {
		return localStorage.getItem(key) ?? undefined;
	} catch {
		return undefined;
	}
}

/** Keeps `value` for `key` in this browser, or forgets it when `value` is undefined. */
export function keep(key: string, value: string | undefined): void {
	try {
		if (value === undefined) localStorage.removeItem(key);
		else localStorage.setItem(key, value);
	} catch {}
}

/** The plural form of a count's words: `one` for 1, else `many` with the count filled in. */
export function counted(
	count: number,
	one: string,
	many: (count: number) => string,
): string {
	return count === 1 ? one : many(count);
}
