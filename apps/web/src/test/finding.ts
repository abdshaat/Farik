import type { FakeSocket } from "./fake-socket.ts";

/** What `finding.status` answers when nothing has happened yet. */
export const IDLE = {
	phase: "idle",
	files_total: 0,
	files_read: 0,
	sessions: 0,
	projects: 0,
	jobs: 0,
	reading: null,
	unreadable: 0,
	error: null,
	started_at: null,
	ended_at: null,
	seconds_left: null,
	paused: false,
	index_exists: false,
	export_dismissed: false,
	version: 0,
};

/** The `finding.list` answer with nothing found. */
export const NOTHING = {
	main: [],
	explored: [],
	gone: [],
	home: { sessions: 0, formed_into: [] },
	computer_jobs: [],
	ignored: [],
};

/** One row of the found list, as the hub answers it. */
export function row(fields: Record<string, unknown>) {
	return {
		key: "/home/me/code/tidepool-web",
		name: "tidepool-web",
		kind: "folder",
		folders: ["/home/me/code/tidepool-web"],
		last_at: "2026-10-22T08:00:00Z",
		jobs: [],
		services: [],
		suggested: "team",
		choice: null,
		already_team: null,
		gone: false,
		...fields,
	};
}

/** An answer that is a refusal, as the hub words one: `<code>: <sentence>`. */
export class Refused {
	message: string;
	constructor(code: string, sentence: string) {
		this.message = `${code}: ${sentence}`;
	}
}

type Answer = unknown | ((params: Record<string, unknown>) => unknown);

/**
 * Answers every call of the methods in `table` the page makes, now and later, the way the hub
 * would: the table is read when the call arrives, so a test changes an answer by changing it. A
 * function gets the call's params; a `Refused` is a refusal.
 */
export function answerCalls(socket: FakeSocket, table: Record<string, Answer>) {
	const seen = new Set<number>();
	let queue: Promise<unknown> = Promise.resolve();
	const handle = (frame: (typeof socket.sent)[number]) => {
		if (seen.has(frame.id) || !(frame.method in table)) return;
		seen.add(frame.id);
		setTimeout(() => {
			const given = table[frame.method];
			const answer =
				typeof given === "function"
					? (given as (p: Record<string, unknown>) => unknown)(frame.params)
					: given;
			queue = queue.then(() =>
				answer instanceof Refused
					? socket.fail(frame, -32005, answer.message)
					: socket.reply(frame, answer),
			);
		}, 0);
	};
	const send = socket.send.bind(socket);
	socket.send = (data: string) => {
		send(data);
		const frame = socket.sent.at(-1);
		if (frame) handle(frame);
	};
	for (const frame of socket.sent) handle(frame);
}
