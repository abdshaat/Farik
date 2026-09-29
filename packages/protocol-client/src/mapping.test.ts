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
});
