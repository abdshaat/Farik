import { describe, expect, it } from "vitest";
import { refusedBy } from "./schema.ts";

describe("refusedBy", () => {
	it("takes_a_body_the_schema_takes", () => {
		expect(
			refusedBy("dataPipelineDecideBody", { pipeline: 7, decision: "approve" }),
		).toEqual([]);
	});

	it("refuses_a_missing_and_an_unknown_field", () => {
		expect(
			refusedBy("dataPipelineDecideBody", { decision: "approve" }),
		).toEqual(["missing pipeline"]);
		expect(
			refusedBy("dataPipelineDecideBody", {
				pipeline: 7,
				decision: "approve",
				reason: "x",
			}),
		).toEqual(["unknown reason"]);
	});

	it("refuses_a_wrong_type_value_or_length", () => {
		expect(
			refusedBy("dataPipelineDecideBody", { pipeline: "7", decision: "maybe" }),
		).toEqual(["pipeline is not an integer", "decision is maybe"]);
		expect(
			refusedBy("dataPipelineDecideBody", {
				pipeline: 7,
				decision: "approve",
				note: "x".repeat(601),
			}),
		).toEqual(["note is too long"]);
	});

	it("refuses_a_text_that_is_empty_on_two_lines_or_too_long", () => {
		const sent = { message: 2, subject: "Delivery date", body: "Hello" };
		expect(refusedBy("sellerMessageSendBody", sent)).toEqual([]);
		expect(
			refusedBy("sellerMessageSendBody", { ...sent, subject: "" }),
		).toEqual(["subject is too short", "subject does not fit its pattern"]);
		expect(
			refusedBy("sellerMessageSendBody", { ...sent, subject: "a\nb" }),
		).toEqual(["subject does not fit its pattern"]);
		expect(
			refusedBy("sellerMessageSendBody", { ...sent, subject: "s".repeat(201) }),
		).toEqual(["subject is too long"]);
		expect(
			refusedBy("sellerMessageSendBody", { ...sent, body: "b".repeat(8001) }),
		).toEqual(["body is too long"]);
		expect(refusedBy("sellerMessageSendBody", { ...sent, body: "" })).toEqual([
			"body is too short",
		]);
	});

	it("refuses_a_number_below_its_minimum", () => {
		expect(refusedBy("sellerMessageDiscardBody", { message: 0 })).toEqual([
			"message is below 1",
		]);
	});

	it("reads_a_rpc_request_s_params_and_the_servers_they_name", () => {
		const params = {
			address: "buying@cornerbakery.test",
			name: "Sam Ortiz",
			provider: "gmail",
			imap: { host: "imap.gmail.com", port: 993, security: "tls" },
			smtp: { host: "smtp.gmail.com", port: 465, security: "tls" },
			username: "buying@cornerbakery.test",
			password: "pw",
			folder: "INBOX",
			signature: "",
			disclose_ai: true,
		};
		expect(refusedBy("procurementMailboxConnectRequest", params)).toEqual([]);
		const { password: _password, ...without } = params;
		expect(
			refusedBy("procurementMailboxConnectRequest", {
				...without,
				disclose_ai_line: true,
				imap: { ...params.imap, port: 0 },
				smtp: { host: "smtp.gmail.com", port: "465", security: "plain" },
			}),
		).toEqual([
			"missing password",
			"imap.port is below 1",
			"smtp.port is not an integer",
			"smtp.security is plain",
			"unknown disclose_ai_line",
		]);
		expect(
			refusedBy("procurementMailboxConnectRequest", {
				...params,
				disclose_ai: "yes",
			}),
		).toEqual(["disclose_ai is not a boolean"]);
	});

	it("names_a_def_that_is_not_there", () => {
		expect(() => refusedBy("sellerMessageSnedBody", {})).toThrow(
			"no schema named sellerMessageSnedBody",
		);
	});
});
