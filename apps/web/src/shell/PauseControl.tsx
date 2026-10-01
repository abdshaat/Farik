import { Button } from "@farik/ui";
import { useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { sentence } from "../app/words.ts";
import { t } from "../strings/t.ts";
import styles from "./Shell.module.css";

/** Pause or Resume the team; the state itself comes from `serve.status`, refreshed by the event. */
export function PauseControl({
	paused,
	short,
}: {
	paused: boolean;
	short: boolean;
}) {
	const { client } = useConnection();
	const [busy, setBusy] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const run = async () => {
		if (!client) return;
		setBusy(true);
		setRefusal(undefined);
		try {
			const reply = await client.command({
				command: paused ? "team_resume" : "team_pause",
				body: {},
			});
			// A race can refuse it (already_paused, not_paused): say so in words.
			if ("error" in reply) setRefusal(sentence(reply.error.detail));
		} catch {
			// The connection closed: the page shows that it is lost.
		} finally {
			setBusy(false);
		}
	};
	const label = short
		? t(paused ? "resume" : "pause")
		: t(paused ? "resumeTeam" : "pauseTeam");
	return (
		<div className={paused ? styles.resume : styles.pause}>
			<Button busy={busy} onClick={run}>
				{label}
			</Button>
			{refusal && (
				<p role="alert" className={styles.refusal}>
					{refusal}
				</p>
			)}
		</div>
	);
}
