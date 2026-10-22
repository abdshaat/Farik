import type { MethodName } from "@catervas/protocol-client";
import { useEffect, useState } from "react";
import { useHub } from "../../app/connection.tsx";
import { said } from "../../app/refusals.ts";
import type { en } from "../../strings/en.ts";
import { t } from "../../strings/t.ts";

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

/** One row of `finding.list`, in camelCase. */
export type FoundRow = {
	/** The project's key: its normalised repository address, its real path, a connector project's name, or a claude.ai uuid. */
	key: string;
	name: string;
	kind: "folder" | "connector" | "claude_ai" | "hand" | "job";
	folders: string[];
	lastAt: string | null;
	jobs: { name: string; schedule: string }[];
	services: string[];
	suggested: Choice | null;
	choice: Choice | null;
	/** The slug of the team whose project folder is one of `folders`. */
	alreadyTeam: string | null;
	gone: boolean;
};

export type Choice = "team" | "watch" | "ignore";

/** `finding.list`'s answer, in camelCase. */
export type FoundList = {
	main: FoundRow[];
	explored: FoundRow[];
	gone: FoundRow[];
	home: { sessions: number; formedInto: string[] };
	computerJobs: FoundRow[];
	ignored: FoundRow[];
};

/**
 * The found list, asked on mount and again whenever the scan publishes new counts (`version`),
 * and on `again()`.
 */
export function useFoundList(version: number | undefined): {
	list: FoundList | undefined;
	again: () => void;
} {
	const { client } = useHub();
	const [list, setList] = useState<FoundList>();
	const [round, setRound] = useState(0);
	// biome-ignore lint/correctness/useExhaustiveDependencies: version and round ask for the list again
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.call("finding.list", {}).then(
			(answer) => live && setList(answer as FoundList),
			() => {},
		);
		return () => {
			live = false;
		};
	}, [client, version, round]);
	return { list, again: () => setRound((n) => n + 1) };
}

/** `path` with the home folder written as `~`: the first two parts of /home/<name> or /Users/<name>. */
export function tilde(path: string): string {
	return path.replace(/^\/(?:home|Users)\/[^/]+(?=\/|$)/, "~");
}

/** The name of a project's folder under Catervas: its name before any bracket, lower case, with dashes. */
export function slugOf(name: string): string {
	return (
		name
			.split("(")[0]
			?.toLowerCase()
			.replace(/[^a-z0-9]+/g, "-")
			.replace(/^-+|-+$/g, "") || "project"
	);
}

/** How long ago `moment` was, in days, as a person says it. */
export function whenWorked(moment: string, now = Date.now()): string {
	const days = Math.floor((now - Date.parse(moment)) / 86_400_000);
	if (days <= 0) return t("foundToday");
	if (days === 1) return t("foundYesterday");
	return t("foundDaysAgo", { count: days });
}

/** A failed call of finding in plain words: its refusal code's sentence, else `other`. */
export function findingSaid(e: unknown, other: keyof typeof en): string {
	const message = e instanceof Error ? e.message : "";
	return said(/^([a-z_]+): /.exec(message)?.[1], {}, other);
}

/** When the owner last opened the found list, kept in this browser: what is newer is "new". */
export const FOUND_SEEN = "catervas.foundSeen";

/** Kept in this browser while the owner chose "I'll do it later" and no export was brought in. */
export const EXPORT_PENDING = "catervas.exportPending";

/** What `POST /hub/export` read: the counts of the conversations and Projects brought in. */
export type ExportSummary = { conversations: number; projects: number };

/** The refusal words of `POST /hub/export`, and the sentence each is read as. */
const EXPORT_WORDS: Record<string, keyof typeof en> = {
	not_an_export: "exportNotAnExport",
	damaged: "exportDamaged",
	unsupported: "exportUnsupported",
	encrypted: "exportEncrypted",
	nothing_dated: "exportNothingDated",
	export_too_large: "exportTooLarge",
};

/**
 * Posts a claude.ai export to the hub, the file itself as the body (it is read on this computer),
 * and answers what came in, or the code the hub refused it with.
 */
export function useExportUpload(): {
	send: (file: File) => Promise<void>;
	busy: boolean;
	summary: ExportSummary | undefined;
	/** The words for why the file was refused; none until it is. */
	refused: keyof typeof en | undefined;
} {
	const [busy, setBusy] = useState(false);
	const [summary, setSummary] = useState<ExportSummary>();
	const [refused, setRefused] = useState<keyof typeof en>();
	const send = async (file: File) => {
		setBusy(true);
		setSummary(undefined);
		setRefused(undefined);
		try {
			const answer = await fetch("/hub/export", {
				method: "POST",
				body: file,
				credentials: "same-origin",
				headers: { "Content-Type": "application/zip" },
			});
			if (answer.ok) {
				const read = (await answer.json()) as ExportSummary;
				setSummary({
					conversations: read.conversations,
					projects: read.projects,
				});
				// The export is in: the card that waited for it is done.
				keep(EXPORT_PENDING, undefined);
			} else {
				const body = (await answer.json().catch(() => ({}))) as {
					error?: string;
				};
				setRefused(EXPORT_WORDS[body.error ?? ""] ?? "exportFailed");
			}
		} catch {
			setRefused("exportFailed");
		} finally {
			setBusy(false);
		}
	};
	return { send, busy, summary, refused };
}

/** A service the owner can tap, from `finding.catalog`. */
export type CatalogService = { id: string; name: string; kind: string };

/** The services of the catalog, asked once. */
export function useCatalog(): CatalogService[] | undefined {
	const { client } = useHub();
	const [services, setServices] = useState<CatalogService[]>();
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.call("finding.catalog", {}).then(
			(answer) =>
				live &&
				setServices((answer as { services: CatalogService[] }).services),
			() => {},
		);
		return () => {
			live = false;
		};
	}, [client]);
	return services;
}

/** Kept in this browser once the owner pressed "Not now" on All teams' offer to find projects. */
export const FIND_NOT_NOW = "catervas.findNotNow";

/** What a method of the hub answers, asked on mount and again on `again()`. */
export function useHubCall<T>(method: MethodName): {
	data: T | undefined;
	again: () => void;
} {
	const { client } = useHub();
	const [data, setData] = useState<T>();
	const [round, setRound] = useState(0);
	// biome-ignore lint/correctness/useExhaustiveDependencies: round asks again
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.call(method, {}).then(
			(answer) => live && setData(answer as T),
			() => {},
		);
		return () => {
			live = false;
		};
	}, [client, method, round]);
	return { data, again: () => setRound((n) => n + 1) };
}
