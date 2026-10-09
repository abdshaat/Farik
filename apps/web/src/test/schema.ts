// The schemas the daemon checks what the web app sends against, read from `docs/schemas`, so that a
// test holds a sent body to what the daemon takes and not only to what the component sends.
// A small reader of the keywords those bodies use: it is no stand-in for the daemon's validator.

import { readFileSync } from "node:fs";
import { join } from "node:path";

type Rule = {
	$ref?: string;
	type?: string;
	enum?: unknown[];
	minLength?: number;
	maxLength?: number;
	minimum?: number;
	maximum?: number;
	pattern?: string;
	required?: string[];
	additionalProperties?: boolean;
	properties?: Record<string, Rule>;
};
type Defs = Record<string, Rule>;

const schemas = (file: string): Defs =>
	(
		JSON.parse(
			readFileSync(
				join(
					import.meta.dirname,
					`../../../../docs/schemas/${file}.schema.json`,
				),
				"utf8",
			),
		) as { $defs: Defs }
	).$defs;

/**
 * The rule a def names: a command's body in `command.schema.json`, else a method's request in
 * `rpc.schema.json`, whose rule is its `params`.
 */
function ruleOf(defName: string): { rule: Rule; defs: Defs } {
	for (const file of ["command", "rpc"]) {
		const defs = schemas(file);
		const found = defs[defName];
		if (!found) continue;
		const params = found.properties?.method ? found.properties.params : found;
		if (params) return { rule: params, defs };
	}
	throw new Error(`no schema named ${defName}`);
}

/** Every way `value` breaks `rule`, each said as `<path> <what>`. */
function breaks(
	rule: Rule,
	value: unknown,
	path: string,
	defs: Defs,
	why: string[],
) {
	if (rule.$ref) {
		const target = defs[rule.$ref.replace("#/$defs/", "")];
		if (target) breaks(target, value, path, defs, why);
		return;
	}
	if (rule.type === "object") {
		if (typeof value !== "object" || value === null || Array.isArray(value)) {
			why.push(`${path} is not an object`);
			return;
		}
		const fields = value as Record<string, unknown>;
		const at = (key: string) => (path === "" ? key : `${path}.${key}`);
		for (const key of rule.required ?? []) {
			if (!(key in fields)) why.push(`missing ${at(key)}`);
		}
		for (const [key, one] of Object.entries(fields)) {
			const inner = rule.properties?.[key];
			if (inner) breaks(inner, one, at(key), defs, why);
			else why.push(`unknown ${at(key)}`);
		}
		return;
	}
	if (rule.type === "integer" && !Number.isInteger(value)) {
		why.push(`${path} is not an integer`);
		return;
	}
	if (rule.type === "string" && typeof value !== "string") {
		why.push(`${path} is not a string`);
		return;
	}
	if (rule.type === "boolean" && typeof value !== "boolean") {
		why.push(`${path} is not a boolean`);
		return;
	}
	if (rule.enum && !rule.enum.includes(value))
		why.push(`${path} is ${String(value)}`);
	if (typeof value === "string") {
		const length = Array.from(value).length;
		if (rule.minLength !== undefined && length < rule.minLength) {
			why.push(`${path} is too short`);
		}
		if (rule.maxLength !== undefined && length > rule.maxLength) {
			why.push(`${path} is too long`);
		}
		if (rule.pattern !== undefined && !new RegExp(rule.pattern).test(value)) {
			why.push(`${path} does not fit its pattern`);
		}
	}
	if (typeof value === "number") {
		if (rule.minimum !== undefined && value < rule.minimum) {
			why.push(`${path} is below ${rule.minimum}`);
		}
		if (rule.maximum !== undefined && value > rule.maximum) {
			why.push(`${path} is above ${rule.maximum}`);
		}
	}
}

/**
 * Why the daemon would refuse `body` as what `defName` defines (a command's body, or a method's
 * params), or nothing when it takes it.
 */
export function refusedBy(
	defName: string,
	body: Record<string, unknown>,
): string[] {
	const { rule, defs } = ruleOf(defName);
	const why: string[] = [];
	breaks(rule, body, "", defs, why);
	return why;
}

/** The body of the command a sent request carries. */
export function bodyOf(sent: { params: unknown }): Record<string, unknown> {
	return (sent.params as { command: { body: Record<string, unknown> } }).command
		.body;
}
