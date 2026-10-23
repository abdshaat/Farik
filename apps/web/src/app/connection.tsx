import {
	connect,
	type DaemonClient,
	type Event,
	type SocketLike,
} from "@catervas/protocol-client";
import {
	createContext,
	type ReactNode,
	useContext,
	useEffect,
	useRef,
	useState,
} from "react";
import { type Due, showDue } from "./notices.ts";

type Status =
	| "checking"
	| "no_session"
	| "connecting"
	| "open"
	| "lost"
	| "reopening";
export type Connection = {
	status: Status;
	client: DaemonClient | null;
	/** The link's code was refused: the link was used before. */
	linkUsed: boolean;
	/** The last events received, oldest first. */
	events: Event[];
	/** Revokes this browser's session and closes the socket; the page then asks for a start link. */
	disconnect: () => Promise<void>;
	/** Catervas is about to restart on the chosen project: its close is expected, and the page reconnects at once. */
	reopen: () => void;
};

const RETRY_MS = 5000;
const REOPEN_RETRY_MS = 500;
const REOPEN_FOR_MS = 30_000;
// ponytail: the event list view needs no more than the last 500.
const KEEP = 500;

const idle: Connection = {
	status: "checking",
	client: null,
	linkUsed: false,
	events: [],
	disconnect: async () => {},
	reopen: () => {},
};
const Context = createContext<Connection>(idle);
/** The hub's connection, which a team's own provider does not replace (ADR 0059). */
const HubContext = createContext<Connection>(idle);
type Factory = (url: string) => SocketLike;
const FactoryContext = createContext<Factory>((url) => new WebSocket(url));

/**
 * A connection: it owns one socket and one event subscription. The page has one around the router
 * for the hub (`/hub/rpc`, the default), and one for the team in the address (`/t/<team>/rpc`,
 * `inner`), which is replaced when the address changes team. The hooks read the nearest.
 */
export function ConnectionProvider(props: {
	children: ReactNode;
	socketFactory?: Factory;
	/** The socket's address; the hub's by default. */
	path?: string;
	/** Inside the hub's provider: the start link is the hub's to trade, not this one's. */
	inner?: boolean;
}) {
	const outer = useContext(FactoryContext);
	const path = props.path ?? "/hub/rpc";
	const inner = props.inner ?? false;
	const [status, setStatus] = useState<Status>("checking");
	const [client, setClient] = useState<DaemonClient | null>(null);
	const [linkUsed, setLinkUsed] = useState(false);
	const [events, setEvents] = useState<Event[]>([]);
	const disconnect = useRef(async () => {});
	const reopen = useRef(() => {});
	const socketFactory = useRef(props.socketFactory ?? outer);

	// The path and `inner` are fixed for a provider: another team's page is a new provider, by key.
	// biome-ignore lint/correctness/useExhaustiveDependencies: see above
	useEffect(() => {
		let stopped = false;
		let lastSeq: number | undefined;
		let retry: ReturnType<typeof setTimeout> | undefined;
		let current: DaemonClient | null = null;
		// While Catervas restarts on a chosen project: until when to keep trying, and whether it was tried yet.
		let reopenUntil: number | undefined;
		let retried = false;
		// The digests already shown as a browser notice.
		const noticed = new Set<string>();

		const lose = () => {
			if (stopped) return;
			current = null;
			setClient(null);
			if (reopenUntil !== undefined && Date.now() < reopenUntil) {
				retry = setTimeout(check, retried ? REOPEN_RETRY_MS : 0);
				retried = true;
				return;
			}
			reopenUntil = undefined;
			setStatus("lost");
			retry = setTimeout(check, RETRY_MS);
		};
		const open = () => {
			const url = `ws://${location.host}${path}`;
			const c = connect(url, socketFactory.current(url));
			current = c;
			setClient(c);
			if (reopenUntil === undefined) setStatus("connecting");
			// The hub tells every page when the daily digest is due; a team's socket is never told.
			if (!inner)
				c.onNotice("reminders.due", (due) => showDue(due as Due, noticed));
			c.onStatus((s) => {
				if (stopped || c !== current) return;
				if (s === "closed") return lose();
				if (s !== "open") return;
				reopenUntil = undefined;
				setStatus("open");
				// The first subscribe starts from now; a reconnect resumes from the last seq held, so
				// that it has no gap and no repeat.
				c.subscribe(lastSeq, (e) => {
					lastSeq = e.seq;
					setEvents((list) => [...list.slice(1 - KEEP), e]);
				})
					.then((head) => {
						lastSeq = Math.max(lastSeq ?? 0, head);
					})
					.catch(() => {}); // a closed socket rejects it, and its close reconnects
			});
		};
		async function check() {
			let answer: Response;
			try {
				answer = await fetch("/session", { credentials: "same-origin" });
			} catch {
				return lose();
			}
			if (stopped) return;
			// A refused WebSocket upgrade gives no status, so the session is asked first.
			// A live session outranks a used link: this browser was let in before.
			if (answer.status === 204) {
				setLinkUsed(false);
				open();
			} else if (answer.status === 401) setStatus("no_session");
			else lose();
		}
		async function redeem() {
			const code = /^#([0-9a-f]{64})$/i.exec(location.hash)?.[1];
			if (code) {
				const sent = fetch("/connect", {
					method: "POST",
					headers: { "Content-Type": "application/json" },
					body: JSON.stringify({ code }),
					credentials: "same-origin",
				});
				// The code is spent either way: keep it out of the history.
				history.replaceState(history.state, "", location.pathname);
				try {
					const answer = await sent;
					if (stopped) return;
					if (answer.status === 204) {
						// The router sits inside this provider; a popstate tells it the path changed.
						history.replaceState(null, "", "/");
						dispatchEvent(new PopStateEvent("popstate"));
					} else setLinkUsed(true);
				} catch {}
			}
		}
		async function start() {
			if (!inner) await redeem();
			await check();
		}
		// A start link opened in this tab only changes the hash.
		const onHash = () => {
			redeem().then(() => {
				if (!stopped && current === null) check();
			});
		};
		if (!inner) addEventListener("hashchange", onHash);
		disconnect.current = async () => {
			const answer = await fetch("/disconnect", {
				method: "POST",
				credentials: "same-origin",
			});
			if (stopped || answer.status !== 204) return;
			// Unset first, so this close is not taken for a lost connection.
			const c = current;
			current = null;
			c?.close();
			setClient(null);
			setStatus("no_session");
		};
		reopen.current = () => {
			reopenUntil = Date.now() + REOPEN_FOR_MS;
			retried = false;
			setStatus("reopening");
		};
		start();
		return () => {
			stopped = true;
			removeEventListener("hashchange", onHash);
			clearTimeout(retry);
			current?.close();
		};
	}, []);

	const value: Connection = {
		status,
		client,
		linkUsed,
		events,
		disconnect: () => disconnect.current(),
		reopen: () => reopen.current(),
	};
	const body = (
		<FactoryContext value={socketFactory.current}>
			<Context value={value}>{props.children}</Context>
		</FactoryContext>
	);
	return inner ? body : <HubContext value={value}>{body}</HubContext>;
}

/** The hub's connection: the computer's, whatever team the address names. */
export function useHub(): Connection {
	return useContext(HubContext);
}

export function useConnection(): Connection {
	return useContext(Context);
}

/** Opens a socket the way this page does; a team's page asks it for a socket of its own. */
export function useSocketFactory(): Factory {
	return useContext(FactoryContext);
}
