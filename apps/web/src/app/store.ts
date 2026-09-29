import type { Event, QueryName, RpcError } from "@farik/protocol-client";
import { useEffect, useRef, useState } from "react";
import { useConnection } from "./connection.tsx";

const REFETCH_MS = 250;

/** The `serve.status` query's answer, in camelCase. */
export type ServeStatus = {
	/** Null while Farik is being set up, before a project is chosen. */
	projectRoot: string | null;
	paused: boolean;
	credential: string | null;
	port: number;
	/** Why taking the chosen project on failed, or null. */
	takeOnError: string | null;
};

/** The last 500 events, oldest first, from the provider's one subscription. */
export function useEvents(): Event[] {
	return useConnection().events;
}

/** Queries on mount and on a new connection, and again after events, at most once per 250 ms, or on `again()`. */
export function useQuery<T>(
	name: QueryName,
	params: object,
): { data: T | undefined; error: RpcError | undefined; again: () => void } {
	const { client, events } = useConnection();
	const [result, setResult] = useState<{
		data: T | undefined;
		error: RpcError | undefined;
	}>({ data: undefined, error: undefined });
	const [round, setRound] = useState(0);
	const key = JSON.stringify(params);
	const lastSeq = events.at(-1)?.seq;
	const seenAtMount = useRef(lastSeq);
	const timer = useRef<ReturnType<typeof setTimeout>>(undefined);

	useEffect(() => {
		if (lastSeq === seenAtMount.current || timer.current) return;
		timer.current = setTimeout(() => {
			timer.current = undefined;
			setRound((n) => n + 1);
		}, REFETCH_MS);
	}, [lastSeq]);
	useEffect(() => () => clearTimeout(timer.current), []);

	// biome-ignore lint/correctness/useExhaustiveDependencies: round asks for the query again
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.query(name, JSON.parse(key)).then(
			(data) => live && setResult({ data: data as T, error: undefined }),
			(error: RpcError) => live && setResult((r) => ({ data: r.data, error })),
		);
		return () => {
			live = false;
		};
	}, [client, name, key, round]);

	return { ...result, again: () => setRound((n) => n + 1) };
}
