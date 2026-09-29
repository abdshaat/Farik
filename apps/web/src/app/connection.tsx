import {
	connect,
	type DaemonClient,
	type Event,
	type SocketLike,
} from "@farik/protocol-client";
import {
	createContext,
	type ReactNode,
	useContext,
	useEffect,
	useRef,
	useState,
} from "react";

type Status = "checking" | "no_session" | "connecting" | "open" | "lost";
type Connection = {
	status: Status;
	client: DaemonClient | null;
	/** The link's code was refused: the link was used before. */
	linkUsed: boolean;
	/** The last events received, oldest first. */
	events: Event[];
	/** Revokes this browser's session and closes the socket; the page then asks for a start link. */
	disconnect: () => Promise<void>;
};

const RETRY_MS = 5000;
// ponytail: the event list view needs no more than the last 500.
const KEEP = 500;

const Context = createContext<Connection>({
	status: "checking",
	client: null,
	linkUsed: false,
	events: [],
	disconnect: async () => {},
});

/** One per page, around the router: it owns the one socket and the one event subscription. */
export function ConnectionProvider(props: {
	children: ReactNode;
	socketFactory?: (url: string) => SocketLike;
}) {
	const [status, setStatus] = useState<Status>("checking");
	const [client, setClient] = useState<DaemonClient | null>(null);
	const [linkUsed, setLinkUsed] = useState(false);
	const [events, setEvents] = useState<Event[]>([]);
	const disconnect = useRef(async () => {});
	const socketFactory = useRef(
		props.socketFactory ?? ((url: string) => new WebSocket(url)),
	);

	useEffect(() => {
		let stopped = false;
		let lastSeq = 0;
		let retry: ReturnType<typeof setTimeout> | undefined;
		let current: DaemonClient | null = null;

		const lose = () => {
			if (stopped) return;
			current = null;
			setClient(null);
			setStatus("lost");
			retry = setTimeout(check, RETRY_MS);
		};
		const open = () => {
			const url = `ws://${location.host}/rpc`;
			const c = connect(url, socketFactory.current(url));
			current = c;
			setClient(c);
			setStatus("connecting");
			c.onStatus((s) => {
				if (stopped || c !== current) return;
				if (s === "closed") return lose();
				if (s !== "open") return;
				setStatus("open");
				// From the last seq held, so a reconnect has no gap and no repeat.
				c.subscribe(lastSeq, (e) => {
					lastSeq = e.seq;
					setEvents((list) => [...list.slice(1 - KEEP), e]);
				}).catch(() => {}); // a closed socket rejects it, and its close reconnects
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
			if (answer.status === 204) open();
			else if (answer.status === 401) setStatus("no_session");
			else lose();
		}
		async function start() {
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
			await check();
		}
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
		start();
		return () => {
			stopped = true;
			clearTimeout(retry);
			current?.close();
		};
	}, []);

	return (
		<Context
			value={{
				status,
				client,
				linkUsed,
				events,
				disconnect: () => disconnect.current(),
			}}
		>
			{props.children}
		</Context>
	);
}

export function useConnection(): Connection {
	return useContext(Context);
}
