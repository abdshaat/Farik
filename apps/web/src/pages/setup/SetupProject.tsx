import { Button, Choice, TextArea, TextField } from "@catervas/ui";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { daemonSaid } from "../../app/refusals.ts";
import { type ServeStatus, useQuery } from "../../app/store.ts";
import { ON_CURRENT } from "../../app/teams.ts";
import {
	AttachButton,
	AttachedChips,
	type AttachKind,
	pendingSetupFiles,
} from "../../components/Attach.tsx";
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
	const { data: serve } = useQuery<ServeStatus>("serve.status", {});
	const [view, setView] = useState<"question" | "project" | "parent">(
		"question",
	);
	const [mode, setMode] = useState<Mode>("existing");
	const [description, setDescription] = useState("");
	const [name, setName] = useState("");
	// The documents picked here are kept until the team exists; the finish screen sends them.
	const [, redraw] = useState(0);
	const picked = pendingSetupFiles.map(({ file }) => ({
		path: file.name,
		name: file.name,
	}));
	const attach = (file: File, kind: AttachKind) => {
		pendingSetupFiles.push({ file, kind });
		redraw((n) => n + 1);
	};
	const unattach = (key: string) => {
		const at = pendingSetupFiles.findIndex(({ file }) => file.name === key);
		if (at >= 0) pendingSetupFiles.splice(at, 1);
		redraw((n) => n + 1);
	};
	const [parent, setParent] = useState("");
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const [opening, setOpening] = useState(false);
	const [replacing, setReplacing] = useState<string>();
	const leaving = serve?.leaving ?? undefined;
	// A project already taken on, its setup not finished: going back here can keep it.
	const keep =
		serve?.projectRoot && serve.setupPending && !leaving
			? serve.projectRoot
			: undefined;
	const links = !leaving && !opening;
	const base = (path: string) => path.split("/").filter(Boolean).at(-1) ?? path;

	// Catervas restarts on the chosen project; once the page is connected to it again, it goes home.
	useEffect(() => {
		if (opening && status === "open")
			navigate("/", { replace: true, state: ON_CURRENT });
	}, [opening, status, navigate]);

	const take = async (
		method: "project.open" | "project.create",
		params: object,
		sandbox = noSandbox(),
	) => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call(method, { ...params, noSandbox: sandbox });
			reopen();
			setOpening(true);
		} catch (e) {
			const message = e instanceof Error ? e.message : "";
			if (message.startsWith("has_team") && method === "project.open")
				setReplacing((params as { path: string }).path);
			else {
				setReplacing(undefined);
				setRefused(daemonSaid(e, "setupRefused"));
			}
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
			<Wizard
				step={2}
				title={t("projectTitle")}
				lead={t("projectLead")}
				links={links}
			>
				<p role="status">{t("opening")}</p>
			</Wizard>
		);
	if (replacing !== undefined)
		return (
			<Wizard
				step={2}
				title={t("projectTitle")}
				lead={t("projectLead")}
				links={links}
			>
				<p>{t("replaceTeam").replace("{target}", base(replacing))}</p>
				<div className={styles.foot}>
					<Button
						onClick={() => {
							setReplacing(undefined);
							setView("project");
						}}
					>
						{t("replaceNo")}
					</Button>
					<Button
						kind="primary"
						busy={busy}
						onClick={() => {
							take("project.open", { path: replacing, replace: true });
						}}
					>
						{t("replaceYes")}
					</Button>
				</div>
			</Wizard>
		);
	if (view !== "question")
		return (
			<Wizard
				step={2}
				title={t(view === "project" ? "browserTitle" : "browserParentTitle")}
				lead={t("browserLead")}
				links={links}
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
		<Wizard
			step={2}
			title={t("projectTitle")}
			lead={t("projectLead")}
			links={links}
		>
			{leaving && (
				<p role="status">{t("movingFrom").replace("{name}", base(leaving))}</p>
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
					<span>
						<AttachButton onPick={attach} />
					</span>
					<AttachedChips items={picked} onRemove={unattach} />
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
				{leaving ? (
					<Button
						busy={busy}
						onClick={() => take("project.open", { path: leaving }, false)}
					>
						{t("stayOn").replace("{name}", base(leaving))}
					</Button>
				) : (
					<Button onClick={() => navigate("/setup/account")}>
						{t("back")}
					</Button>
				)}
				<span className={styles.buttons}>
					<Button
						kind={keep ? "secondary" : "primary"}
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
					{keep && (
						<Button kind="primary" onClick={() => navigate("/setup/scan")}>
							{t("projectKeep").replace("{name}", base(keep))}
						</Button>
					)}
				</span>
			</div>
		</Wizard>
	);
}
