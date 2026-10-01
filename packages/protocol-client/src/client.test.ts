import { describe, expect, it } from "vitest";
import { connect, RpcError, type SocketLike } from "./client.ts";

class FakeSocket implements SocketLike {
	sent: Record<string, unknown>[] = [];
	private handlers: Record<string, ((e: { data: unknown }) => void)[]> = {};
	send(data: string) {
		this.sent.push(JSON.parse(data));
	}
	close() {
		this.emit("close", {});
	}
	addEventListener(type: string, fn: (e: { data: unknown }) => void) {
		const list = this.handlers[type] ?? [];
		this.handlers[type] = list;
		list.push(fn);
	}
	emit(type: string, e: unknown) {
		for (const fn of this.handlers[type] ?? []) fn(e as { data: unknown });
	}
	receive(frame: object) {
		this.emit("message", { data: JSON.stringify(frame) });
	}
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("client", () => {
	it("numbers_requests_and_matches_responses", async () => {
		const ws = new FakeSocket();
		const client = connect("ws://x/rpc", ws);
		ws.emit("open", {});
		const a = client.query("tasks.list", {});
		const b = client.query("team.get", {});
		await tick();
		expect(ws.sent.map((f) => f.id)).toEqual([1, 2]);
		expect(ws.sent[0]).toMatchObject({
			jsonrpc: "2.0",
			method: "query",
			params: { name: "tasks.list", params: {} },
		});
		ws.receive({ jsonrpc: "2.0", id: 2, result: { team: { some_key: "b" } } });
		ws.receive({ jsonrpc: "2.0", id: 1, result: { tasks: [] } });
		expect(await a).toEqual({ tasks: [] });
		expect(await b).toEqual({ team: { someKey: "b" } });
	});

	it("rejects_with_the_error_code", async () => {
		const ws = new FakeSocket();
		const client = connect("ws://x/rpc", ws);
		const p = client.query("tasks.list", {});
		await tick();
		ws.receive({
			jsonrpc: "2.0",
			id: 1,
			error: { code: -32601, message: "no such method" },
		});
		const err = await p.catch((e: unknown) => e);
		expect(err).toBeInstanceOf(RpcError);
		expect((err as RpcError).code).toBe(-32601);
		expect((err as RpcError).message).toBe("no such method");
	});

	it("keeps_a_refusals_data_on_the_error", async () => {
		const ws = new FakeSocket();
		const client = connect("ws://x/rpc", ws);
		const p = client.call("team.save", { team: {} });
		await tick();
		const errors = [{ path: "/agents", message: "m", code: "too_many" }];
		ws.receive({
			jsonrpc: "2.0",
			id: 1,
			error: { code: -32005, message: "m", data: { errors } },
		});
		const err = (await p.catch((e: unknown) => e)) as RpcError;
		expect(err.data).toEqual({ errors });
	});

	it("delivers_event_notifications_in_camel_case", async () => {
		const ws = new FakeSocket();
		const client = connect("ws://x/rpc", ws);
		const seen: unknown[] = [];
		const sub = client.subscribe(0, (e) => seen.push(e));
		await tick();
		expect(ws.sent[0]).toMatchObject({
			method: "subscribe",
			params: { from_seq: 0 },
		});
		ws.receive({ jsonrpc: "2.0", id: 1, result: {} });
		await sub;
		ws.receive({
			jsonrpc: "2.0",
			method: "event",
			params: { event: { seq: 1, recorded_at: "t", body: {} } },
		});
		expect(seen).toEqual([{ seq: 1, recordedAt: "t", body: {} }]);
	});

	it("reports_its_status", () => {
		const ws = new FakeSocket();
		const client = connect("ws://x/rpc", ws);
		const seen: string[] = [];
		client.onStatus((s) => seen.push(s));
		ws.emit("open", {});
		ws.emit("close", {});
		expect(seen).toEqual(["connecting", "open", "closed"]);
	});

	it("call sends a method and resolves its result", async () => {
		const ws = new FakeSocket();
		const client = connect("ws://x/rpc", ws);
		ws.emit("open", {});
		const p = client.call("project.open", { path: "code/a", noSandbox: true });
		await tick();
		expect(ws.sent[0]).toMatchObject({
			jsonrpc: "2.0",
			method: "project.open",
			params: { path: "code/a", no_sandbox: true },
		});
		ws.receive({ jsonrpc: "2.0", id: 1, result: { project_root: "/h/a" } });
		expect(await p).toEqual({ projectRoot: "/h/a" });
	});

	it("maps_saved_teams_and_the_template_a_team_came_from", async () => {
		const ws = new FakeSocket();
		const client = connect("ws://x/rpc", ws);
		ws.emit("open", {});
		const seen: unknown[] = [];
		const sub = client.subscribe(0, (e) => seen.push(e));
		const listed = client.query("templates.list", {});
		const applied = client.call("template.apply", { slug: "pair" });
		await tick();
		expect(ws.sent[2]).toMatchObject({
			method: "template.apply",
			params: { slug: "pair" },
		});
		ws.receive({ jsonrpc: "2.0", id: 1, result: {} });
		await sub;
		ws.receive({
			jsonrpc: "2.0",
			id: 2,
			result: {
				folder: "/h/.config/farik/templates",
				templates: [
					{ slug: "pair", template: { name: "Pair", saved_at: "t" } },
				],
				unreadable: [],
			},
		});
		ws.receive({ jsonrpc: "2.0", id: 3, result: { added: ["noor"] } });
		ws.receive({
			jsonrpc: "2.0",
			method: "event",
			params: {
				event: {
					seq: 9,
					kind: "team.updated",
					body: { team_name: "Farik", template: "Pair" },
				},
			},
		});
		expect(await listed).toEqual({
			folder: "/h/.config/farik/templates",
			templates: [{ slug: "pair", template: { name: "Pair", savedAt: "t" } }],
			unreadable: [],
		});
		expect(await applied).toEqual({ added: ["noor"] });
		expect(seen).toEqual([
			{
				seq: 9,
				kind: "team.updated",
				body: { teamName: "Farik", template: "Pair" },
			},
		]);
	});

	it("maps_the_backlog_and_the_sprint_policy", async () => {
		const ws = new FakeSocket();
		const client = connect("ws://x/rpc", ws);
		ws.emit("open", {});
		const summary = client.query("backlog.summary", {});
		const listed = client.query("tasks.list", {});
		await tick();
		expect(ws.sent[0]).toMatchObject({
			method: "query",
			params: { name: "backlog.summary", params: {} },
		});
		ws.receive({
			jsonrpc: "2.0",
			id: 1,
			result: { plan_in_sprints: true, count: 2 },
		});
		ws.receive({
			jsonrpc: "2.0",
			id: 2,
			result: { tasks: [{ task_id: "FRK-1", backlog: true }] },
		});
		expect(await summary).toEqual({ planInSprints: true, count: 2 });
		expect(await listed).toEqual({
			tasks: [{ taskId: "FRK-1", backlog: true }],
		});
	});
});
