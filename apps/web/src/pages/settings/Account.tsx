import { Button } from "@catervas/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { daemonSaid } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import {
	type CheckAnswer,
	ClaudeSignIn,
	outcomeWords,
} from "../ClaudeSignIn.tsx";
import styles from "../pages.module.css";

type AccountStatus = {
	signedIn: boolean;
	kind: "subscription" | "console" | "environment" | "other" | null;
	/** The variable an account from the environment comes from, which Catervas cannot sign out. */
	environmentVariable?: string;
	/** Whether the provider refused the key, so the team is paused until a sign-in or a check works. */
	keyRefused?: boolean;
	/** Whether this start removed the key an earlier version kept. */
	keptKeyRemoved?: boolean;
};

/** The AI account's row: what Claude Code is signed in with, a check, Sign out (asked twice), and signing in again. */
export function Account() {
	const { client } = useConnection();
	const { data, again } = useQuery<AccountStatus>("account.status", {});
	const [asking, setAsking] = useState(false);
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();

	const check = async () => {
		if (!client) return;
		setBusy(true);
		setSaid(undefined);
		try {
			setSaid(
				outcomeWords((await client.call("account.check", {})) as CheckAnswer),
			);
		} catch (e) {
			setSaid(daemonSaid(e, "signInFailed"));
		}
		again();
		setBusy(false);
	};
	const signOut = async () => {
		if (!client) return;
		setBusy(true);
		try {
			await client.call("account.sign_out", {});
			setSaid(t("accountSignedOut"));
		} catch (e) {
			setSaid(daemonSaid(e, "signOutFailed"));
		}
		setAsking(false);
		again();
		setBusy(false);
	};
	const signedBack = () => {
		setSaid(t("accountSignedBack"));
		again();
	};

	const variable = data?.environmentVariable;
	const kindWords = (kind: NonNullable<AccountStatus["kind"]>) =>
		t(
			(
				{
					subscription: "accountSubscription",
					console: "accountConsole",
					other: "accountOther",
					environment: "accountOther",
				} as const
			)[kind],
		);
	return (
		<section className={styles.section} aria-labelledby="account-heading">
			<h2 id="account-heading">{t("accountRow")}</h2>
			{data?.keptKeyRemoved && (
				<p role="status">{t("accountKeptKeyRemoved")}</p>
			)}
			{data?.keyRefused && (
				<p role="alert">
					{variable
						? t("accountKeyRefusedEnvironment", { variable })
						: t("connectionKeyRefused")}
				</p>
			)}
			{data &&
				(data.signedIn && data.kind ? (
					<>
						<p>
							{data.kind === "environment"
								? t("accountSignedIn", {
										kind: t("signedInEnvironment", {
											variable: variable ?? "",
										}).replace(/^Claude Code is signed in with /, ""),
									})
								: t("accountSignedIn", { kind: kindWords(data.kind) })}
						</p>
						{variable && (
							<p className={styles.muted}>{t("accountKept", { variable })}</p>
						)}
						<div className={styles.actions}>
							<Button busy={busy} onClick={check}>
								{t("accountCheck")}
							</Button>
							{!variable && !asking && (
								<Button onClick={() => setAsking(true)}>
									{t("accountSignOut")}
								</Button>
							)}
						</div>
						{asking && (
							<>
								<p>{t("accountConfirm")}</p>
								<div className={styles.actions}>
									<Button busy={busy} onClick={signOut}>
										{t("accountSignOutYes")}
									</Button>
									<Button kind="quiet" onClick={() => setAsking(false)}>
										{t("agentCancel")}
									</Button>
								</div>
							</>
						)}
					</>
				) : (
					<p>{t("accountNone")}</p>
				))}
			{data && !variable && (!data.signedIn || data.keyRefused) && (
				<>
					<h3>{t("accountSignInAgain")}</h3>
					<ClaudeSignIn onWorking={signedBack} />
				</>
			)}
			{said && <p role="status">{said}</p>}
		</section>
	);
}
