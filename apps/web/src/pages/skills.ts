import type { DaemonClient } from "@farik/protocol-client";
import { RpcError } from "@farik/protocol-client";
import { useEffect, useState } from "react";
import { useConnection } from "../app/connection.tsx";

/** A skill's folder as `skill.get` answers it. */
export type SkillFolder = {
	files: Record<string, string>;
	sha256: string;
	ignoredFields: string[];
};

/** Which skill: the team's, or one agent's. */
export type SkillAt = { level: "team" | "agent"; name: string };

/** The `skill.get` params for `skill` of `agent`. */
export const skillParams = (agent: string, skill: SkillAt) => ({
	level: skill.level,
	...(skill.level === "agent" && { agent }),
	name: skill.name,
});

/** The `skill_*` code a refusal's message starts with, or "" when it starts with none. */
export const codeOf = (message: string) =>
	/^([a-z_]+)(?::|$)/.exec(message)?.[1] ?? "";

/** The code of a failed `skill.get`, or "" when it carries none. */
export const refusedWith = (e: unknown) =>
	codeOf(e instanceof RpcError || e instanceof Error ? e.message : "");

/** Sends a skill command; the refusal's code ("" when it has none) or undefined once it is done. */
export async function sendSkill(
	client: DaemonClient,
	command: object,
): Promise<string | undefined> {
	try {
		const reply = await client.command(command as never);
		return "error" in reply ? codeOf(reply.error.detail) : undefined;
	} catch {
		// The connection closed: the page shows that it is lost.
		return "";
	}
}

/** The folder of `skill`, read once: loading, the folder, or the code that refused it. */
export function useSkillFolder(agent: string, skill: SkillAt) {
	const { client } = useConnection();
	const { level, name } = skill;
	const [got, setGot] = useState<SkillFolder | { refused: string }>();
	useEffect(() => {
		if (!client) return;
		let live = true;
		client.query("skill.get", skillParams(agent, { level, name })).then(
			(folder) => live && setGot(folder as SkillFolder),
			(e) => live && setGot({ refused: refusedWith(e) }),
		);
		return () => {
			live = false;
		};
	}, [client, agent, level, name]);
	return got;
}

/** A size in KB, at least 1. */
export const kb = (text: string) =>
	Math.max(1, Math.ceil(new TextEncoder().encode(text).length / 1024));
