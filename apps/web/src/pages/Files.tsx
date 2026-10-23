import { Avatar, type AvatarKey, RoleTag } from "@catervas/ui";
import { useParams } from "react-router";
import { Link, useNavigate } from "../app/links.tsx";
import { useQuery } from "../app/store.ts";
import { useWide } from "../shell/Shell.tsx";
import { t } from "../strings/t.ts";
import {
	type Catervafication,
	CatervaficationCard,
} from "./CatervaficationCard.tsx";
import { FileDocument } from "./FileDocument.tsx";
import styles from "./Files.module.css";
import { type FilesTree, filePath, fileRoute } from "./files.ts";
import type { Agent, Team } from "./setup/TeamSetup.tsx";

/** The team's folders and their documents (spec 4.5): the tree beside the open document, on a phone a picker above it. */
export function Files() {
	const splat = useParams()["*"];
	const wide = useWide();
	const { data: team } = useQuery<{ team: Team; documentsMode?: string }>(
		"team.get",
		{},
	);
	const { data: tree, again } = useQuery<FilesTree>("files.tree", {});
	const { data: documenting, again: askDocumentingAgain } =
		useQuery<Catervafication>("catervafication.get", {});
	const agents = team?.team.agents;
	// Nothing shows until the team and the tree are both here: the owner's face is part of every row.
	if (!tree || !agents) return null;
	const all = tree.folders.flatMap((folder) =>
		folder.documents.map((document) => ({ folder, document })),
	);
	// `/files` shows the first document of the first folder that has one.
	const path = splat ? filePath(splat) : all[0]?.document.path;
	const open = all.find((one) => one.document.path === path);
	const shown = open ? (
		<FileDocument
			key={open.document.path}
			tree={tree}
			path={open.document.path}
			agents={agents}
			mode={team?.documentsMode ?? "committed"}
			onSaved={again}
		/>
	) : (
		splat && <h2 className={styles.none}>{t("filesNoSuch")}</h2>
	);
	// The document sits in the same place in both layouts, so that a draft survives a resize.
	return (
		<div className={wide ? styles.wide : styles.narrow}>
			{documenting?.offered && (
				<div className={styles.offer}>
					<CatervaficationCard again={askDocumentingAgain} />
				</div>
			)}
			{wide ? (
				<FolderTree tree={tree} agents={agents} open={path} />
			) : (
				<div className={styles.phoneHead}>
					<Link to="/team" className={styles.back}>
						{t("filesBack")}
					</Link>
					<h1 className={styles.title}>{t("files")}</h1>
					<Picker
						tree={tree}
						agents={agents}
						open={path}
						folder={open?.folder ?? tree.folders[0]}
					/>
				</div>
			)}
			<div className={styles.document}>{shown}</div>
		</div>
	);
}

/** A folder's owner, by the folder's `agentId`. */
const ownerOf = (agents: Agent[], id: string) =>
	agents.find((a) => a.id === id);

function Face({ agent }: { agent: Agent | undefined }) {
	return agent?.avatar ? (
		<span aria-hidden="true">
			<Avatar
				avatarKey={agent.avatar as AvatarKey}
				name={agent.displayName}
				size={32}
			/>
		</span>
	) : null;
}

function FolderTree({
	tree,
	agents,
	open,
}: {
	tree: FilesTree;
	agents: Agent[];
	open: string | undefined;
}) {
	return (
		<nav aria-labelledby="files-title" className={styles.tree}>
			<h1 id="files-title" className={styles.title}>
				{t("files")}
			</h1>
			<p className={styles.lead}>{t("filesLead")}</p>
			<ul className={styles.folders}>
				{tree.folders.map((folder) => {
					const owner = ownerOf(agents, folder.agentId);
					const who = (
						<>
							<Face agent={owner} />
							<span className={styles.name}>
								<span className={styles.top}>
									<RoleTag role={folder.role} label={folder.name} />
									<span className={styles.owner}>{owner?.displayName}</span>
								</span>
								{folder.documents.length === 0 && (
									<span className={styles.empty}>{t("filesNothingHere")}</span>
								)}
							</span>
						</>
					);
					return (
						<li key={folder.folder}>
							{folder.documents.length === 0 ? (
								<div className={`${styles.fold} ${styles.nil}`}>{who}</div>
							) : (
								<details open={folder.documents.some((d) => d.path === open)}>
									<summary className={styles.fold}>{who}</summary>
									<ul className={styles.docs}>
										{folder.documents.map((document) => (
											<li key={document.path}>
												<Link
													to={fileRoute(document.path)}
													aria-current={
														document.path === open ? "page" : undefined
													}
												>
													{document.name}
												</Link>
											</li>
										))}
									</ul>
								</details>
							)}
						</li>
					);
				})}
			</ul>
		</nav>
	);
}

/** A phone's picker: one option for each document, a folder with none one disabled option. */
function Picker({
	tree,
	agents,
	open,
	folder,
}: {
	tree: FilesTree;
	agents: Agent[];
	open: string | undefined;
	folder: FilesTree["folders"][number] | undefined;
}) {
	const navigate = useNavigate();
	const owner = folder && ownerOf(agents, folder.agentId);
	return (
		<div className={styles.pick}>
			<label htmlFor="files-pick">{t("filesFile")}</label>
			<select
				id="files-pick"
				value={open ?? ""}
				onChange={(e) => navigate(fileRoute(e.target.value))}
			>
				{tree.folders.map((one) => (
					<optgroup
						key={one.folder}
						label={`${one.name}, ${ownerOf(agents, one.agentId)?.displayName ?? ""}`}
					>
						{one.documents.length === 0 ? (
							<option disabled>
								{t("filesEmptyOption", { folder: one.name })}
							</option>
						) : (
							one.documents.map((document) => (
								<option key={document.path} value={document.path}>
									{t("filesCrumb", { folder: one.name, name: document.name })}
								</option>
							))
						)}
					</optgroup>
				))}
			</select>
			{owner && folder && (
				<span className={styles.pickOwner}>
					<Face agent={owner} />
					{t("filesOwnerFolder", {
						owner: owner.displayName,
						folder: folder.name,
					})}
				</span>
			)}
		</div>
	);
}
