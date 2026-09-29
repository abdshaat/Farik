import { Button, Choice, TextField } from "@farik/ui";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";
import { Wizard } from "./Wizard.tsx";

type Kind = "subscription_token" | "api_key";
const SETUP_TOKEN = "claude setup-token";

/** Setup's second step: the key Farik's agents use, kept in the keychain or a private file. */
export function SetupAccount() {
	const { client, status, reopen } = useConnection();
	const navigate = useNavigate();
	const { data: account } = useQuery<{ source: string | null }>(
		"account.status",
		{},
	);
	const [kind, setKind] = useState<Kind>("subscription_token");
	const [secret, setSecret] = useState("");
	const [saving, setSaving] = useState(false);
	const [refused, setRefused] = useState<string>();
	const [opening, setOpening] = useState(false);
	const subscription = kind === "subscription_token";

	const save = async () => {
		if (!client) return;
		setSaving(true);
		setRefused(undefined);
		try {
			const answer = (await client.call("account.connect", {
				kind,
				secret,
			})) as { storedIn: "keychain" | "file"; takingOn: boolean };
			// A project that waited on the key is being taken on: Farik restarts on it.
			if (answer.takingOn) {
				reopen();
				setOpening(true);
				return;
			}
			// The next screen says where the key went.
			navigate("/setup/project", { state: { stored: answer.storedIn } });
		} catch (e) {
			setRefused((e as Error).message);
			setSaving(false);
		}
	};
	const done = secret === "" && !!account?.source;

	// Once the page is connected to the restarted Farik, it goes home.
	useEffect(() => {
		if (opening && status === "open") navigate("/", { replace: true });
	}, [opening, status, navigate]);

	if (opening)
		return (
			<Wizard step={1} title={t("accountTitle")} lead={t("accountLead")}>
				<p role="status">{t("opening")}</p>
			</Wizard>
		);

	return (
		<Wizard step={1} title={t("accountTitle")} lead={t("accountLead")}>
			<div className={styles.card}>
				<Choice<Kind>
					name="kind"
					legend={t("accountKind")}
					value={kind}
					onChange={(k) => {
						setKind(k);
						setRefused(undefined);
					}}
					options={[
						{
							value: "subscription_token",
							label: t("subscription"),
							description: t("subscriptionNote"),
						},
						{
							value: "api_key",
							label: t("apiKey"),
							description: t("apiKeyNote"),
						},
					]}
				/>
			</div>
			<div className={styles.card}>
				{subscription && (
					<ol>
						<li>
							{t("subscriptionStepType")}
							<span className={styles.copy}>
								<code>{SETUP_TOKEN}</code>
								<Button
									kind="quiet"
									onClick={() => navigator.clipboard?.writeText(SETUP_TOKEN)}
								>
									{t("copy")}
								</Button>
							</span>
						</li>
						<li>{t("subscriptionStepPaste")}</li>
					</ol>
				)}
				<TextField
					id="secret"
					type="password"
					label={t(subscription ? "subscriptionKey" : "apiKeyField")}
					hint={t(subscription ? "subscriptionHint" : "apiKeyHint")}
					value={secret}
					onChange={setSecret}
					{...(refused && { error: refused })}
				/>
			</div>
			{account?.source && <p>{t("accountFound")}</p>}
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/computer")}>{t("back")}</Button>
				{done ? (
					<Button kind="primary" onClick={() => navigate("/setup/project")}>
						{t("continue")}
					</Button>
				) : (
					<Button
						kind="primary"
						busy={saving}
						disabled={secret.trim() === ""}
						onClick={save}
					>
						{t("saveContinue")}
					</Button>
				)}
			</div>
		</Wizard>
	);
}
