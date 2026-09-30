import { Button, Choice, Switch } from "@farik/ui";
import { useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { type ServeStatus, useQuery } from "../app/store.ts";
import type { ThemeChoice } from "../app/theme.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";

const ADVANCED = "farik.advanced";

// Storage can refuse (a private window): the switch then lives only in memory.
function readAdvanced(): boolean {
	try {
		return localStorage.getItem(ADVANCED) === "true";
	} catch {
		return false;
	}
}

/** The Advanced switch, shared by every page that shows detailed settings. */
export function useAdvanced(): [boolean, (on: boolean) => void] {
	const [advanced, setAdvanced] = useState(readAdvanced);
	const toggle = (on: boolean) => {
		try {
			localStorage.setItem(ADVANCED, String(on));
		} catch {}
		setAdvanced(on);
	};
	return [advanced, toggle];
}

export function Settings({
	theme,
	onTheme,
}: {
	theme: ThemeChoice;
	onTheme: (c: ThemeChoice) => void;
}) {
	const { disconnect } = useConnection();
	const { data } = useQuery<ServeStatus>("serve.status", {});
	const [advanced, toggle] = useAdvanced();
	const [leaving, setLeaving] = useState(false);
	const leave = async () => {
		setLeaving(true);
		try {
			await disconnect();
		} catch {
			// The request failed: this browser is still connected, and the button can be tried again.
		} finally {
			setLeaving(false);
		}
	};
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("settings")}</h1>
			<Choice<ThemeChoice>
				name="theme"
				legend={t("theme")}
				value={theme}
				onChange={onTheme}
				options={[
					{
						value: "light",
						label: t("themeLight"),
						description: t("themeLightNote"),
					},
					{
						value: "dark",
						label: t("themeDark"),
						description: t("themeDarkNote"),
					},
					{
						value: "system",
						label: t("themeSystem"),
						description: t("themeSystemNote"),
					},
				]}
			/>
			<section className={styles.section} aria-labelledby="advanced-heading">
				<h2 id="advanced-heading">{t("advanced")}</h2>
				<Switch
					id="advanced"
					label={t("advancedSwitch")}
					description={t("advancedBody")}
					checked={advanced}
					onChange={toggle}
				/>
				<p>{t("advancedSafe")}</p>
			</section>
			<section className={styles.section} aria-labelledby="computer-heading">
				<h2 id="computer-heading">{t("thisComputer")}</h2>
				<p>
					<strong>{t("connectedLead")}</strong> {t("connectedBody")}
				</p>
				{data && (
					<dl className={styles.facts}>
						<dt>{t("projectFolder")}</dt>
						<dd>{data.projectRoot}</dd>
						<dt>{t("address")}</dt>
						<dd>{`127.0.0.1:${data.port}`}</dd>
					</dl>
				)}
				<div>
					<Button busy={leaving} onClick={leave}>
						{t("disconnect")}
					</Button>
				</div>
			</section>
			<Account />
			<section className={styles.section} aria-labelledby="language-heading">
				<h2 id="language-heading">{t("language")}</h2>
				<p>{t("english")}</p>
				<p className={styles.muted}>{t("englishOnly")}</p>
			</section>
		</div>
	);
}

type AccountStatus = {
	provider: string | null;
	kind: "api_key" | "subscription_token" | null;
	source: "environment" | "keychain" | "file" | null;
};
type Disconnected = {
	removedFrom: string[];
	paused: boolean;
	environmentVariable?: string;
};

/** The AI account's row: what is connected and where it is kept, and Disconnect. */
function Account() {
	const { client } = useConnection();
	const { data } = useQuery<AccountStatus>("account.status", {});
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();
	const disconnect = async () => {
		if (!client) return;
		setBusy(true);
		try {
			const gone = (await client.call(
				"account.disconnect",
				{},
			)) as Disconnected;
			setSaid(
				gone.environmentVariable
					? t("accountKept").replace("{variable}", gone.environmentVariable)
					: t("accountGone"),
			);
		} catch (e) {
			setSaid((e as Error).message);
		}
		setBusy(false);
	};
	return (
		<section className={styles.section} aria-labelledby="account-heading">
			<h2 id="account-heading">{t("accountRow")}</h2>
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
						{t(
							(
								{
									keychain: "accountKeychain",
									file: "accountFile",
									environment: "accountEnvironment",
								} as const
							)[data.source],
						)}
					</p>
					{!said && (
						<div>
							<Button busy={busy} onClick={disconnect}>
								{t("accountDisconnect")}
							</Button>
						</div>
					)}
				</>
			) : (
				data && <p>{t("accountNone")}</p>
			)}
			{said && <p role="status">{said}</p>}
		</section>
	);
}
