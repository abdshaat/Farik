import type { CatervasCommand } from "./generated/command.ts";
import type { CatervasEvent } from "./generated/event.ts";
import type { QueryRequest } from "./generated/rpc.ts";
import { toCamel, toSnake } from "./mapping.ts";

type Camel<T> = T extends readonly (infer U)[]
	? Camel<U>[]
	: T extends object
		? {
				[K in keyof T as K extends string ? CamelKey<K> : K]: Camel<T[K]>;
			}
		: T;
type CamelKey<S extends string> = S extends `${infer A}_${infer B}`
	? `${A}${Capitalize<CamelKey<B>>}`
	: S;

/** An event as the browser sees it: the wire's fields in camelCase. */
export type Event = Camel<CatervasEvent>;
/** A command as the browser writes it: the wire's fields in camelCase. */
export type Command = Camel<CatervasCommand>;
export type QueryName = QueryRequest["params"]["name"];
export type CommandReply =
	| { said: string; events: number[] }
	| {
			error: {
				kind: "invalid" | "refused" | "not_found" | "failed";
				detail: string;
			};
	  };
export type MethodName =
	| "project.open"
	| "project.create"
	| "project.leave"
	| "keys_copied.dismiss"
	| "account.connect"
	| "sandbox.build"
	| "browser.pull"
	| "request.file"
	| "contract.save"
	| "team.save"
	| "agent.replace"
	| "team.start"
	| "criteria.save"
	| "account.disconnect"
	| "project.note"
	| "template.save"
	| "template.apply"
	| "template.rename"
	| "template.delete"
	| "connector.tools"
	| "connector.connect"
	| "connector.allowances"
	| "connector.disconnect"
	| "connector.sign_in"
	| "connector.sign_in_status"
	| "connector.sign_in_cancel"
	| "social_post.media"
	| "purchase_order.file"
	| "seller_reply.attachment"
	| "procurement_mailbox.connect"
	| "procurement_mailbox.disconnect"
	| "procurement_mailbox.check"
	| "marketing_budget.raise";
export type Status = "connecting" | "open" | "closed";

/** The part of the browser `WebSocket` the client uses. */
export interface SocketLike {
	readyState?: number;
	send(data: string): void;
	close(): void;
	addEventListener(type: string, fn: (e: { data: unknown }) => void): void;
}

export class RpcError extends Error {
	code: number;
	/** What the daemon gave with a refusal, as it gave it: `errors`, each a path, a message and a code. */
	data: unknown;
	constructor(code: number, message: string, data?: unknown) {
		super(message);
		this.name = "RpcError";
		this.code = code;
		this.data = data;
	}
}

export type DaemonClient = {
	subscribe(fromSeq: number, onEvent: (e: Event) => void): Promise<void>;
	command(c: Command): Promise<CommandReply>;
	query(name: QueryName, params: object): Promise<unknown>;
	call(method: MethodName, params: object): Promise<unknown>;
	onStatus(cb: (s: Status) => void): void;
	close(): void;
};

type Pending = { resolve: (v: unknown) => void; reject: (e: RpcError) => void };

export function connect(url: string, socket?: SocketLike): DaemonClient {
	const ws: SocketLike = socket ?? new WebSocket(url);
	const pending = new Map<number, Pending>();
	const queued: string[] = [];
	const listeners: ((s: Status) => void)[] = [];
	const handlers: ((e: Event) => void)[] = [];
	let nextId = 1;
	let status: Status = "connecting";

	const setStatus = (s: Status) => {
		status = s;
		for (const cb of listeners) cb(s);
	};
	ws.addEventListener("open", () => {
		setStatus("open");
		for (const frame of queued.splice(0)) ws.send(frame);
	});
	ws.addEventListener("close", () => {
		setStatus("closed");
		for (const p of pending.values())
			p.reject(new RpcError(-1, "connection closed"));
		pending.clear();
	});
	ws.addEventListener("message", (e: { data: unknown }) => {
		const frame = JSON.parse(String(e.data)) as {
			id?: number | null;
			result?: unknown;
			error?: { code: number; message: string; data?: unknown };
			method?: string;
			params?: { event: unknown };
		};
		if (frame.method === "event" && frame.params) {
			const event = toCamel(frame.params.event) as Event;
			for (const h of handlers) h(event);
			return;
		}
		const p = frame.id == null ? undefined : pending.get(frame.id);
		if (!p || frame.id == null) return;
		pending.delete(frame.id);
		if (frame.error)
			p.reject(
				new RpcError(frame.error.code, frame.error.message, frame.error.data),
			);
		else p.resolve(toCamel(frame.result));
	});

	const request = (method: string, params: unknown): Promise<unknown> =>
		new Promise((resolve, reject) => {
			const id = nextId++;
			pending.set(id, { resolve, reject });
			const frame = JSON.stringify({
				jsonrpc: "2.0",
				id,
				method,
				params: toSnake(params),
			});
			// readyState 0 is CONNECTING: a real socket throws on send until it opens.
			if (ws.readyState === 0) queued.push(frame);
			else ws.send(frame);
		});

	return {
		async subscribe(fromSeq, onEvent) {
			handlers.push(onEvent);
			await request("subscribe", { fromSeq });
		},
		command: (command) =>
			request("command", { command }) as Promise<CommandReply>,
		query: (name, params) => request("query", { name, params }),
		call: (method, params) => request(method, params),
		onStatus(cb) {
			listeners.push(cb);
			cb(status);
		},
		close: () => ws.close(),
	};
}
