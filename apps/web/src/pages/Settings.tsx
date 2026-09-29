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

export function Settings({
	theme,
	onTheme,
}: {
	theme: ThemeChoice;
	onTheme: (c: ThemeChoice) => void;
}) {
	const { disconnect } = useConnection();
	const { data } = useQuery<ServeStatus>("serve.status", {});
	const [advanced, setAdvanced] = useState(readAdvanced);
	const [leaving, setLeaving] = useState(false);
	const toggle = (on: boolean) => {
		try {
			localStorage.setItem(ADVANCED, String(on));
		} catch {}
		setAdvanced(on);
	};
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
			<section className={styles.section} aria-labelledby="language-heading">
				<h2 id="language-heading">{t("language")}</h2>
				<p>{t("english")}</p>
				<p className={styles.muted}>{t("englishOnly")}</p>
			</section>
		</div>
	);
}
