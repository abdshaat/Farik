import { Button } from "@catervas/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { saidAll } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import { ConnectAgain } from "../ConnectAgain.tsx";
import styles from "../pages.module.css";

type AccountStatus = {
	provider: string | null;
	kind: "api_key" | "subscription_token" | null;
	source: "environment" | "keychain" | "file" | null;
	/** The variable a credential from the environment comes from, which Catervas cannot remove. */
	environmentVariable?: string;
	/** Whether the provider refused the key, so the team is paused until it is connected again. */
	keyRefused?: boolean;
};

/** The AI account's row: what is connected and where it is kept, and Disconnect, asked twice. */
export function Account() {
	const { client } = useConnection();
	const { data, again } = useQuery<AccountStatus>("account.status", {});
	const [asking, setAsking] = useState(false);
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();
	const disconnect = async () => {
		if (!client) return;
		setBusy(true);
		try {
			await client.call("account.disconnect", {});
			setSaid(t("accountGone"));
			again();
		} catch (e) {
			setSaid(saidAll(e));
		}
		setAsking(false);
		setBusy(false);
	};
	const variable = data?.environmentVariable;
	return (
		<section className={styles.section} aria-labelledby="account-heading">
			<h2 id="account-heading">{t("accountRow")}</h2>
			{data?.keyRefused &&
				(data.environmentVariable ? (
					<p role="alert">
						{t("accountKeyRefusedEnvironment").replace(
							"{variable}",
							data.environmentVariable,
						)}
					</p>
				) : (
					<ConnectAgain onConnected={again} />
				))}
			{data?.source && data.kind ? (
				<>
					<p>
						{t("accountConnected").replace(
							"{kind}",
							t(
								data.kind === "api_key"
									? "accountApiKey"
									: "accountSubscription",
							),
						)}
					</p>
					<p className={styles.muted}>
						{variable
							? t("accountKept").replace("{variable}", variable)
							: t(
									(
										{
											keychain: "accountKeychain",
											file: "accountFile",
											environment: "accountEnvironment",
										} as const
									)[data.source],
								)}
					</p>
					{!variable && !said && !asking && (
						<div>
							<Button onClick={() => setAsking(true)}>
								{t("accountDisconnect")}
							</Button>
						</div>
					)}
					{asking && (
						<>
							<p>{t("accountConfirm")}</p>
							<div className={styles.actions}>
								<Button busy={busy} onClick={disconnect}>
									{t("accountDisconnectYes")}
								</Button>
								<Button kind="quiet" onClick={() => setAsking(false)}>
									{t("agentCancel")}
								</Button>
							</div>
						</>
					)}
				</>
			) : (
				data && <p>{t("accountNone")}</p>
			)}
			{said && <p role="status">{said}</p>}
		</section>
	);
}
