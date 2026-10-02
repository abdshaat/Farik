import { describe, expect, it } from "vitest";
import { toCamel, toSnake } from "./mapping.ts";

describe("mapping", () => {
	it("maps_keys_to_camel_case_and_back", () => {
		const wire = { task_id: 1, body: { from_seq: 2 }, xs: [{ a_b: 1 }] };
		const camel = { taskId: 1, body: { fromSeq: 2 }, xs: [{ aB: 1 }] };
		expect(toCamel(wire)).toEqual(camel);
		expect(toSnake(camel)).toEqual(wire);
		expect(toCamel([{ a_b: 1 }, { c_d: 2 }])).toEqual([{ aB: 1 }, { cD: 2 }]);
	});

	it("leaves_tool_input_and_output_alone", () => {
		const input = '{"file_path":"a.rs","old_string":"x"}';
		const event = {
			recorded_at: "t",
			body: { input, output: '{"exit_code":0}' },
		};
		expect(toCamel(event)).toEqual({
			recordedAt: "t",
			body: { input, output: '{"exit_code":0}' },
		});
	});

	it("keeps_the_names_a_user_gave_tools_keys_and_headers", () => {
		// A connector's tools, keys and headers are named by the user or the server, not the wire.
		const camel = {
			agent: "theo",
			server: {
				credentialKeys: ["API_KEY"],
				headers: { Authorization: "Bearer {API_KEY}" },
			},
			keys: { API_KEY: "v" },
			tags: { list_bases: "network", listBases: "denied" },
		};
		expect(toSnake(camel)).toEqual({
			agent: "theo",
			server: {
				credential_keys: ["API_KEY"],
				headers: { Authorization: "Bearer {API_KEY}" },
			},
			keys: { API_KEY: "v" },
			tags: { list_bases: "network", listBases: "denied" },
		});
		const wire = {
			stored_in: "file",
			tools: { list_bases: "network" },
			team: {
				agents: [
					{
						mcp_servers: [
							{
								tools: { list_bases: "network", "describe-table": "denied" },
								headers: { "X-Api-Key": "{API_KEY}" },
							},
						],
					},
				],
			},
		};
		expect(toCamel(wire)).toEqual({
			storedIn: "file",
			tools: { list_bases: "network" },
			team: {
				agents: [
					{
						mcpServers: [
							{
								tools: { list_bases: "network", "describe-table": "denied" },
								headers: { "X-Api-Key": "{API_KEY}" },
							},
						],
					},
				],
			},
		});
		// A list of tools is still a list of wire objects.
		expect(toCamel({ tools: [{ name: "a_b", is_x: true }] })).toEqual({
			tools: [{ name: "a_b", isX: true }],
		});
	});

	it("maps_the_sign_in_calls_and_keeps_the_scopes_as_given", () => {
		// What `connector.sign_in` takes, and what it and `team.get` answer (ADR 0033).
		const camel = {
			agent: "theo",
			server: {
				name: "notion",
				oauth: {
					clientId: "abc",
					callbackPort: 33418,
					scopes: ["read", "offline_access"],
				},
			},
			attempt: "0123",
		};
		const wire = {
			agent: "theo",
			server: {
				name: "notion",
				oauth: {
					client_id: "abc",
					callback_port: 33418,
					scopes: ["read", "offline_access"],
				},
			},
			attempt: "0123",
		};
		expect(toSnake(camel)).toEqual(wire);
		expect(toCamel(wire)).toEqual(camel);
		expect(
			toCamel({
				attempt: "0123",
				authorize_url: "https://auth.example/authorize",
				issuer: "https://auth.example",
			}),
		).toEqual({
			attempt: "0123",
			authorizeUrl: "https://auth.example/authorize",
			issuer: "https://auth.example",
		});
		expect(
			toCamel({
				connectors: [
					{ server: "notion", auth: "oauth", revokes: true, stored_in: "file" },
				],
			}),
		).toEqual({
			connectors: [
				{ server: "notion", auth: "oauth", revokes: true, storedIn: "file" },
			],
		});
	});
});
