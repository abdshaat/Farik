import { Button, Dialog, TextField } from "@farik/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { refusalsOf, said } from "../../app/refusals.ts";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { andList, Faces } from "../SavedTeams.tsx";
import type { Agent } from "../setup/TeamSetup.tsx";

/**
 * "Save as a template": a name, then `template.save`. A name another saved team holds turns Save
 * into Replace, which sends `replace: true`; changing the name turns it back.
 */
export function SaveTemplate({
	agents,
	onSaved,
	onClose,
}: {
	agents: Agent[];
	onSaved: (name: string) => void;
	onClose: () => void;
}) {
	const { client } = useConnection();
	const [name, setName] = useState("");
	const [taken, setTaken] = useState(false);
	const [refused, setRefused] = useState<string>();
	const [busy, setBusy] = useState(false);
	const save = async () => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("template.save", {
				name,
				...(taken && { replace: true }),
			});
			onSaved(name.trim());
		} catch (e) {
			const code = refusalsOf(e)[0]?.code;
			setTaken(code === "template_exists");
			setRefused(said(code, { name: name.trim() }));
		}
		setBusy(false);
	};
	return (
		<Dialog
			open
			title={t("saveTitle")}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("agentCancel")}</Button>
					<Button kind="primary" busy={busy} onClick={save}>
						{t(taken ? "saveReplace" : "saveSave")}
					</Button>
				</>
			}
		>
			<p>{t("saveLead")}</p>
			<p className={styles.cardHead}>
				<Faces agents={agents} />
				<span className={styles.muted}>
					{andList(agents.map((a) => a.displayName))}
				</span>
			</p>
			<p>
				<strong>{t("saveKeeps")}</strong>
				<br />
				{t("saveKeepsBody")}
			</p>
			<p>
				<strong>{t("saveStays")}</strong>
				<br />
				{t("saveStaysBody")}
			</p>
			<TextField
				id="template-name"
				label={t("saveName")}
				hint={t("saveNameHint")}
				value={name}
				onChange={(next) => {
					setName(next);
					setTaken(false);
					setRefused(undefined);
				}}
			/>
			{refused && (
				<p role="alert" className={styles.alert}>
					{refused}
				</p>
			)}
		</Dialog>
	);
}
