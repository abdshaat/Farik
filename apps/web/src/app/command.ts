import { useState } from "react";
import { useConnection } from "./connection.tsx";
import { commandSaid } from "./refusals.ts";

/**
 * Sends one command; closes on success, else keeps its refusal in words (`fill` fills the words'
 * `{placeholders}`) and the code the daemon refused it with.
 */
export function useCommand(
	onDone: () => void,
	fill: Record<string, string> = {},
) {
	const { client } = useConnection();
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const [code, setCode] = useState<string>();
	const send = async (command: object) => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		setCode(undefined);
		try {
			const reply = await client.command(command as never);
			if ("error" in reply) {
				setRefusal(commandSaid(reply.error.detail, fill));
				setCode(/^([a-z_]+): /.exec(reply.error.detail)?.[1]);
			} else onDone();
		} catch {
			// The connection closed: the page shows that it is lost.
		} finally {
			setBusy(false);
		}
	};
	return { busy, refusal, code, send };
}
