import { Avatar, type AvatarKey, Button, TextArea } from "@catervas/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { refusalsOf, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { Markdown } from "../components/Markdown.tsx";
import { t } from "../strings/t.ts";
import styles from "./Files.module.css";
import {
	type FileRead,
	type FilesTree,
	type FileVersion,
	type Integration,
	timeAgo,
} from "./files.ts";
import type { Agent } from "./setup/TeamSetup.tsx";

/** What the editor holds: the owner's draft and the blob it was written against. */
type Draft = { text: string; base: string; preview: boolean };

const SAVING = {
	auto_merge: "filesSaveAuto",
	pull_request: "filesSavePullRequest",
	manual: "filesSaveManual",
} as const;
const WAITING = {
	auto_merge: "filesWaitingAuto",
	pull_request: "filesWaitingPullRequest",
	manual: "filesWaitingManual",
} as const;

/** "Changed by Mira 5 days ago, in CTV-9": no line when nobody changed the version. */
function Changed({
	last,
	agents,
}: {
	last: FileVersion["lastChange"];
	agents: Agent[];
}) {
	if (!last) return null;
	const who =
		agents.find((a) => a.id === last.agentId)?.displayName ??
		(last.byYou ? t("filesYou") : last.author);
	return (
		<p className={styles.changed}>
			{t("filesChanged", { who, when: timeAgo(last.at, new Date()) })}
			{last.taskId && (
				<>
					{t("filesChangedIn")}
					<Link to={`/tasks/${last.taskId}`}>{last.taskId}</Link>
				</>
			)}
		</p>
	);
}

/** What saving does, by the team's integration policy; for a pair, who then updates the team's version. */
function whatSaving(integration: Integration, owner: string | null) {
	if (owner === null) return t(SAVING[integration]);
	const then = t(integration === "auto_merge" ? "filesThenAuto" : "filesThen", {
		owner,
	});
	return `${t(SAVING[integration])} ${then}`;
}

/**
 * One document of the tree, in the version for you or, for a pair, the team's (spec 4.5). The
 * owner's draft and its base are this component's own state, so the page's re-queries after events
 * never touch them.
 */
export function FileDocument({
	tree,
	path,
	agents,
	onSaved,
}: {
	tree: FilesTree;
	path: string;
	agents: Agent[];
	onSaved: () => void;
}) {
	const { client } = useConnection();
	const { data, again } = useQuery<FileRead>("files.read", { path });
	const [team, setTeam] = useState(false);
	const [draft, setDraft] = useState<Draft | null>(null);
	const [refusal, setRefusal] = useState<string | "changed" | null>(null);
	const [busy, setBusy] = useState(false);
	const folder = tree.folders.find((f) =>
		f.documents.some((d) => d.path === path),
	);
	const document = folder?.documents.find((d) => d.path === path);
	// An answer for the page before this one is not this page's.
	if (!data || data.document.path !== path || !folder || !document) return null;
	const owner = agents.find((a) => a.id === folder.agentId);
	const pair = data.twin !== null;
	const version = team && data.twin ? data.twin : data.document;

	const edit = () => {
		setRefusal(null);
		setDraft({
			text: data.document.text,
			base: data.document.blob,
			preview: false,
		});
	};
	const save = async () => {
		if (!draft || !client) return;
		setBusy(true);
		try {
			await client.call("files.save", {
				path,
				text: draft.text,
				base: draft.base,
			});
			setDraft(null);
			setRefusal(null);
			again();
			onSaved();
		} catch (e) {
			setRefusal(
				refusalsOf(e).some((r) => r.code === "file_changed")
					? "changed"
					: saidAll(e),
			);
		} finally {
			setBusy(false);
		}
	};
	// The owner's text stays; the new version's text and blob replace it.
	const reload = async () => {
		if (!client) return;
		try {
			const fresh = (await client.query("files.read", { path })) as FileRead;
			setDraft({
				text: fresh.document.text,
				base: fresh.document.blob,
				preview: false,
			});
			setRefusal(null);
			again();
		} catch (e) {
			setRefusal(saidAll(e));
		}
	};

	const name = document.name;
	// A document's own first-level heading is its title, so the page shows it once, not twice.
	const first = /^# +(.+?)(?: +#+)? *\r?\n/.exec(version.text);
	const title = first?.[1] ?? name;
	const body = first ? version.text.slice(first[0].length) : version.text;
	return (
		<div className={styles.doc}>
			<p className={styles.crumb}>
				{t("filesCrumb", { folder: folder.name, name })}
			</p>
			<div className={styles.head}>
				<div className={styles.headText}>
					<h2 className={styles.title}>{title}</h2>
					{draft ? (
						pair && <p className={styles.changed}>{t("filesEditing")}</p>
					) : (
						<Changed last={version.lastChange} agents={agents} />
					)}
					{!draft && !pair && data.editable && (
						<p className={styles.forTeam}>{t("filesWrittenForTeam")}</p>
					)}
				</div>
				{!draft &&
					!team &&
					data.editable &&
					(data.waiting ? (
						<p className={styles.forTeam}>
							{t(WAITING[tree.integration], { change: data.waiting.change })}
						</p>
					) : (
						<Button onClick={edit}>{t("filesEdit")}</Button>
					))}
			</div>
			{draft ? (
				<>
					{refusal === "changed" ? (
						<div role="alert" className={`${styles.alert} ${styles.alertRow}`}>
							<span>{t("filesChangedWhile")}</span>
							<Button onClick={reload}>{t("filesReload")}</Button>
						</div>
					) : (
						refusal && (
							<p role="alert" className={styles.alert}>
								{refusal}
							</p>
						)
					)}
					<fieldset
						className={styles.version}
						aria-label={t("filesEditOrPreview")}
					>
						<button
							type="button"
							aria-pressed={!draft.preview}
							onClick={() => setDraft({ ...draft, preview: false })}
						>
							{t("filesWrite")}
						</button>
						<button
							type="button"
							aria-pressed={draft.preview}
							onClick={() => setDraft({ ...draft, preview: true })}
						>
							{t("filesPreview")}
						</button>
					</fieldset>
					{draft.preview ? (
						<Markdown text={draft.text} />
					) : (
						<TextArea
							id="file-text"
							label={t("filesText", { name })}
							hint={t("filesHint")}
							value={draft.text}
							onChange={(text) => setDraft({ ...draft, text })}
							rows={24}
						/>
					)}
					<div className={styles.editActs}>
						<Button
							kind="primary"
							busy={busy}
							disabled={refusal === "changed"}
							onClick={save}
						>
							{t("filesSave")}
						</Button>
						<Button kind="quiet" onClick={() => setDraft(null)}>
							{t("filesCancel")}
						</Button>
						<p>
							{whatSaving(
								tree.integration,
								pair ? (owner?.displayName ?? "") : null,
							)}
						</p>
					</div>
				</>
			) : (
				<>
					{pair && (
						<fieldset
							className={styles.version}
							aria-label={t("filesWhichVersion")}
						>
							<button
								type="button"
								aria-pressed={!team}
								onClick={() => setTeam(false)}
							>
								{t("filesForYou")}
							</button>
							<button
								type="button"
								aria-pressed={team}
								onClick={() => setTeam(true)}
							>
								{t("filesForTeam")}
							</button>
						</fieldset>
					)}
					{team && data.stale && (
						<p
							className={`${styles.notice} ${data.rederiving ? styles.work : ""}`}
						>
							{data.rederiving ? (
								<>
									{owner?.avatar && (
										<span aria-hidden="true">
											<Avatar
												avatarKey={owner.avatar as AvatarKey}
												name={owner.displayName}
												size={32}
											/>
										</span>
									)}
									<span className={styles.bringing}>
										{t("filesBringing", { owner: owner?.displayName ?? "" })}
									</span>
								</>
							) : (
								t("filesOlder")
							)}
						</p>
					)}
					{team && (
						<p className={styles.forTeam}>
							{t("filesTeamReads", {
								name: name.charAt(0).toLowerCase() + name.slice(1),
								owner: owner?.displayName ?? "",
							})}
						</p>
					)}
					<Markdown text={body} />
				</>
			)}
		</div>
	);
}
