import { mkdirSync, writeFileSync } from "node:fs";
import { compileFromFile } from "json-schema-to-typescript";

const schemas = new URL("../../../docs/schemas/", import.meta.url);
const out = new URL("./generated/", import.meta.url);

export async function generate(): Promise<void> {
	mkdirSync(out, { recursive: true });
	for (const name of ["rpc", "event", "command"]) {
		const ts = await compileFromFile(
			new URL(`${name}.schema.json`, schemas).pathname,
		);
		writeFileSync(new URL(`${name}.ts`, out), ts);
	}
}

if (import.meta.main) await generate();
