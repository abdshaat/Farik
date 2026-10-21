import { Button, TextField } from "@catervas/ui";
import { useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { daemonSaid } from "../app/refusals.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

/** What `account.check` and `account.sign_in_code` answer, in camelCase. */
export type CheckAnswer = {
	outcome:
		| "working"
		| "key_refused"
		| "limit_reached"
		| "not_signed_in"
		| "failed";
	detail?: string;
	/** In the first run: a project that waited on the account is being taken on. */
	takingOn?: boolean;
};

/** The words a person reads for a check's outcome. */
export function outcomeWords(answer: CheckAnswer): string {
	switch (answer.outcome) {
		case "working":
			return t("connectionWorks");
		case "key_refused":
			return t("connectionKeyRefused");
		case "limit_reached":
			return t("connectionLimitReached");
		case "not_signed_in":
			return t("connectionNotSignedIn");
		case "failed":
			return t("connectionFailed", { detail: answer.detail ?? "" });
	}
}

/**
 * Signing in through Claude Code, in the first run and in Settings: two choices (a Claude
 * subscription, or an Anthropic Console account); the one pressed opens Claude's page in a new tab
 * and waits for the code that page shows, which goes to Claude Code and is kept nowhere. A sign-in
 * that works is told to `onWorking`; any other outcome is said here.
 */
export function ClaudeSignIn({
	onWorking,
}: {
	onWorking: (answer: CheckAnswer) => void;
}) {
	const { client } = useConnection();
	const [url, setUrl] = useState<string>();
	const [blocked, setBlocked] = useState(false);
	const [code, setCode] = useState("");
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();
	const [refused, setRefused] = useState<string>();

	const start = async (consoleAccount: boolean) => {
		if (!client) return;
		setBusy(true);
		setSaid(undefined);
		setRefused(undefined);
		try {
			const answer = (await client.call("account.sign_in", {
				console: consoleAccount,
			})) as { url: string };
			// Opened in the press's own handler, so that a popup blocker allows it.
			setBlocked(window.open(answer.url, "_blank", "noopener") === null);
			setUrl(answer.url);
		} catch (e) {
			setRefused(daemonSaid(e, "signInNoAddress"));
		}
		setBusy(false);
	};

	const answered = (answer: CheckAnswer) => {
		if (answer.outcome === "working") onWorking(answer);
		else setSaid(outcomeWords(answer));
	};

	const finish = async () => {
		if (!client) return;
		const typed = code.trim();
		setBusy(true);
		setRefused(undefined);
		try {
			const answer = (await client.call("account.sign_in_code", {
				code: typed,
			})) as CheckAnswer;
			setCode("");
			setUrl(undefined);
			answered(answer);
		} catch (e) {
			// The sign-in is spent whatever the refusal: it starts again from the choices.
			setCode("");
			setUrl(undefined);
			setRefused(daemonSaid(e, "signInFailed"));
		}
		setBusy(false);
	};

	const checkAgain = async () => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			answered((await client.call("account.check", {})) as CheckAnswer);
		} catch (e) {
			setRefused(daemonSaid(e, "signInFailed"));
		}
		setBusy(false);
	};

	return (
		<>
			{said && <p role="status">{said}</p>}
			{refused && <p role="alert">{refused}</p>}
			{url === undefined ? (
				<div>
					<div className={styles.actions}>
						<Button kind="primary" busy={busy} onClick={() => start(false)}>
							{t("signInClaude")}
						</Button>
					</div>
					<p className={styles.muted}>{t("signInClaudeNote")}</p>
					<div className={styles.actions}>
						<Button busy={busy} onClick={() => start(true)}>
							{t("signInConsole")}
						</Button>
					</div>
					<p className={styles.muted}>{t("signInConsoleNote")}</p>
					<p className={styles.muted}>{t("signInTerminal")}</p>
					<div className={styles.actions}>
						<Button kind="quiet" busy={busy} onClick={checkAgain}>
							{t("signInCheckAgain")}
						</Button>
					</div>
				</div>
			) : (
				<div>
					<p>{t("signInOnThePage")}</p>
					<TextField
						id="sign-in-code"
						type="password"
						autoComplete="off"
						label={t("signInCode")}
						value={code}
						onChange={setCode}
					/>
					<div className={styles.actions}>
						<Button
							kind="primary"
							busy={busy}
							disabled={code.trim() === ""}
							onClick={finish}
						>
							{t("signInFinish")}
						</Button>
						<a href={url} target="_blank" rel="noopener noreferrer">
							{t(blocked ? "signInOpenPage" : "signInOpenAgain")}
						</a>
					</div>
					<p className={styles.muted}>{t("signInTerminal")}</p>
					<div className={styles.actions}>
						<Button kind="quiet" busy={busy} onClick={checkAgain}>
							{t("signInCheckAgain")}
						</Button>
					</div>
				</div>
			)}
		</>
	);
}
