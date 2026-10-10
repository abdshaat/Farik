import { Avatar, type AvatarKey, Button } from "@catervas/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useQuery } from "../app/store.ts";
import { Markdown } from "../components/Markdown.tsx";
import { t } from "../strings/t.ts";
import styles from "./Files.module.css";
import {
	type FileRead,
	type FilesTree,
	type FileVersion,
	timeAgo,
} from "./files.ts";
import type { Agent } from "./setup/TeamSetup.tsx";

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
					<Link to={`/tasks/${last.taskId}`} className={styles.task}>
						{last.taskId}
					</Link>
				</>
			)}
		</p>
	);
}

/** One document of the tree, in the version for you or, for a pair, the team's (spec 4.5). */
export function FileDocument({
	tree,
	path,
	agents,
}: {
	tree: FilesTree;
	path: string;
	agents: Agent[];
}) {
	const { data } = useQuery<FileRead>("files.read", { path });
	const [team, setTeam] = useState(false);
	const folder = tree.folders.find((f) =>
		f.documents.some((d) => d.path === path),
	);
	const document = folder?.documents.find((d) => d.path === path);
	// An answer for the page before this one is not this page's.
	if (!data || data.document.path !== path || !folder || !document) return null;
	const owner = agents.find((a) => a.id === folder.agentId);
	const pair = data.twin !== null;
	const version = team && data.twin ? data.twin : data.document;
	return (
		<div className={styles.doc}>
			<p className={styles.crumb}>
				{t("filesCrumb", { folder: folder.name, name: document.name })}
			</p>
			<div className={styles.head}>
				<div className={styles.headText}>
					<h2 className={styles.title}>{document.name}</h2>
					<Changed last={version.lastChange} agents={agents} />
					{!pair && data.editable && (
						<p className={styles.forTeam}>{t("filesWrittenForTeam")}</p>
					)}
				</div>
				{!team && data.editable && <Button>{t("filesEdit")}</Button>}
			</div>
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
				<p className={`${styles.notice} ${data.rederiving ? styles.work : ""}`}>
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
						name:
							document.name.charAt(0).toLowerCase() + document.name.slice(1),
						owner: owner?.displayName ?? "",
					})}
				</p>
			)}
			<Markdown text={version.text} />
		</div>
	);
}
