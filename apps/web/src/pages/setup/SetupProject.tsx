import { Button, Choice, TextArea, TextField } from "@catervas/ui";
import { useEffect, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { daemonSaid } from "../../app/refusals.ts";
import { type ServeStatus, useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import { FolderBrowser, join } from "./FolderBrowser.tsx";
import styles from "./setup.module.css";
import { noSandbox, Wizard } from "./Wizard.tsx";

type Mode = "existing" | "new";

/** A project's folder name: lowercase letters, digits and `-`, at most 64 characters. */
export function slug(name: string): string {
	return name
		.normalize("NFKD")
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.slice(0, 64)
		.replace(/^-+|-+$/g, "");
}

/** Setup's third step: an existing git folder from home, or a new project Catervas makes. */
export function SetupProject() {
	const { client, status, reopen } = useConnection();
	const navigate = useNavigate();
	const stored = (useLocation().state as { stored?: string } | null)?.stored;
	const { data: serve } = useQuery<ServeStatus>("serve.status", {});
	const [view, setView] = useState<"question" | "project" | "parent">(
		"question",
	);
	const [mode, setMode] = useState<Mode>("existing");
	const [description, setDescription] = useState("");
	const [name, setName] = useState("");
	const [parent, setParent] = useState("");
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const [opening, setOpening] = useState(false);

	// Catervas restarts on the chosen project; once the page is connected to it again, it goes home.
	useEffect(() => {
		if (opening && status === "open") navigate("/", { replace: true });
	}, [opening, status, navigate]);

	const take = async (
		method: "project.open" | "project.create",
		params: object,
	) => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call(method, { ...params, noSandbox: noSandbox() });
			reopen();
			setOpening(true);
		} catch (e) {
			setRefused(daemonSaid(e, "setupRefused"));
			setBusy(false);
		}
	};
	const folder = slug(name);
	const refusal = refused && (
		<p role="alert" className={styles.alert}>
			{refused}
		</p>
	);

	if (opening)
		return (
			<Wizard step={2} title={t("projectTitle")} lead={t("projectLead")}>
				<p role="status">{t("opening")}</p>
			</Wizard>
		);
	if (view !== "question")
		return (
			<Wizard
				step={2}
				title={t(view === "project" ? "browserTitle" : "browserParentTitle")}
				lead={t("browserLead")}
			>
				<FolderBrowser
					mode={view}
					busy={busy}
					onUse={(path) => {
						if (view === "project") return take("project.open", { path });
						setParent(path);
						setView("question");
					}}
				/>
				{refusal}
				{view === "project" && (
					<p className={styles.note}>{t("browserNote")}</p>
				)}
				<div className={styles.foot}>
					<Button onClick={() => setView("question")}>{t("back")}</Button>
				</div>
			</Wizard>
		);
	return (
		<Wizard step={2} title={t("projectTitle")} lead={t("projectLead")}>
			{stored && (
				<p role="status">
					{t(stored === "keychain" ? "storedKeychain" : "storedFile")}
				</p>
			)}
			{serve?.takeOnError && (
				<p role="alert" className={styles.alert}>
					{t("takeOnError")} {serve.takeOnError}
				</p>
			)}
			<div className={styles.card}>
				<Choice<Mode>
					name="project"
					legend={t("projectChoice")}
					value={mode}
					onChange={setMode}
					options={[
						{
							value: "existing",
							label: t("projectExisting"),
							description: t("projectExistingNote"),
						},
						{
							value: "new",
							label: t("projectNew"),
							description: t("projectNewNote"),
						},
					]}
				/>
			</div>
			{mode === "new" && (
				<div className={styles.form}>
					<TextArea
						id="description"
						label={t("newWhat")}
						hint={t("newWhatHint")}
						value={description}
						onChange={setDescription}
					/>
					<TextField
						id="name"
						label={t("newName")}
						{...(folder && {
							hint: t("newNameHint").replace(
								"{folder}",
								`~/${join(parent, folder)}`,
							),
						})}
						value={name}
						onChange={setName}
					/>
					<div className={styles.where}>
						<span>{t("newWhere")}</span>
						<code>{`~/${parent}`}</code>
						<Button onClick={() => setView("parent")}>{t("choose")}</Button>
					</div>
				</div>
			)}
			{refusal}
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/account")}>{t("back")}</Button>
				<Button
					kind="primary"
					busy={busy}
					disabled={mode === "new" && (!folder || !description.trim())}
					onClick={() =>
						mode === "existing"
							? setView("project")
							: take("project.create", { parent, name: folder, description })
					}
				>
					{t("continue")}
				</Button>
			</div>
		</Wizard>
	);
}
