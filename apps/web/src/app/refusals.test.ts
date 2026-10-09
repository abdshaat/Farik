import { RpcError } from "@farik/protocol-client";
import { describe, expect, it } from "vitest";
import { en } from "../strings/en.ts";
import { commandSaid, refusalsOf, said } from "./refusals.ts";

describe("refusals", () => {
	it("words_the_refusals_of_mail_that_a_person_can_act_on", () => {
		// None of these falls to the sentence for a refusal that is no one's to fix.
		expect(said("mailbox_settings_invalid")).toBe(en.refuseMailboxSettings);
		expect(said("seller_message_too_long")).toBe(en.refuseSellerMessageTooLong);
		expect(said("seller_message_field_invalid")).toBe(
			en.refuseSellerMessageField,
		);
		expect(said("mailbox_files")).toBe(en.refuseMailboxFiles);
		expect(said("secret_store_unavailable")).toBe(en.refuseSecretStore);
		expect(said("seller_message_closed")).toBe(en.refuseSellerMessageClosed);
		for (const key of [
			"refuseMailboxSettings",
			"refuseSellerMessageTooLong",
			"refuseSellerMessageField",
			"refuseMailboxFiles",
			"refuseSecretStore",
			"refuseSellerMessageClosed",
		] as const) {
			expect(en[key]).toBeTruthy();
			expect(en[key]).not.toBe(en.refuseOther);
		}
	});

	it("words_each_refusal_by_its_code_and_nothing_raw", () => {
		expect(said("too_many")).toBe(en.refuseTooMany);
		expect(said("too_short")).toBe(en.requestTooShort);
		expect(said("judge_not_held", { role: "Architect" })).toBe(
			en.refuseJudge.replaceAll("{role}", "Architect"),
		);
		// A schema's own text never reaches the screen.
		expect(said("invalid")).toBe(en.refuseOther);
		expect(said(undefined)).toBe(en.refuseOther);

		const errors = [
			{ path: "/agents", message: "has more than 7 items", code: "too_many" },
		];
		expect(refusalsOf(new RpcError(-32005, "x", { errors }))).toEqual(errors);
		expect(
			refusalsOf(new RpcError(-32602, "the params are not right")),
		).toEqual([{ path: "", message: "the params are not right" }]);

		expect(
			commandSaid("last_of_role: theo is your only Software Developer", {
				name: "Theo",
				role: "Developer",
			}),
		).toBe(
			en.refuseLastOfRole
				.replaceAll("{name}", "Theo")
				.replaceAll("{role}", "Developer"),
		);
		expect(commandSaid("not_a_question: event 7 is a task.created", {})).toBe(
			en.refuseOther,
		);
	});
});
