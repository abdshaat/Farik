import { Button, Choice, TextField } from "@farik/ui";
import { useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { daemonSaid } from "../app/refusals.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

type Kind = "subscription_token" | "api_key";

/** Settings' account row while the provider refuses the key: a new key, kept, resumes the team. */
export function ConnectAgain({ onConnected }: { onConnected: () => void }) {
	const { client } = useConnection();
	const [kind, setKind] = useState<Kind>("subscription_token");
	const [secret, setSecret] = useState("");
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const subscription = kind === "subscription_token";
	const connect = async () => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("account.connect", { kind, secret });
			setSecret("");
			onConnected();
		} catch (e) {
			setRefused(daemonSaid(e, "setupRefused"));
		}
		setBusy(false);
	};
	return (
		<>
			<p role="alert">{t("accountKeyRefused")}</p>
			<Choice<Kind>
				name="kind"
				legend={t("accountKind")}
				value={kind}
				onChange={setKind}
				options={[
					{ value: "subscription_token", label: t("subscription") },
					{ value: "api_key", label: t("apiKey") },
				]}
			/>
			<TextField
				id="secret"
				type="password"
				label={t(subscription ? "subscriptionKey" : "apiKeyField")}
				hint={t(subscription ? "subscriptionHint" : "apiKeyHint")}
				value={secret}
				onChange={setSecret}
				{...(refused && { error: refused })}
			/>
			<div className={styles.actions}>
				<Button
					kind="primary"
					busy={busy}
					disabled={secret.trim() === ""}
					onClick={connect}
				>
					{t("accountConnectAgain")}
				</Button>
			</div>
		</>
	);
}
