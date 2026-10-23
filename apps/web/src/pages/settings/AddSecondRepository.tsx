import { Button, Dialog, TextField } from "@catervas/ui";
import { useState } from "react";
import { useConnection } from "../../app/connection.tsx";
import { refusalsOf, said } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import styles from "../find/find.module.css";
import { FolderBrowser } from "../setup/FolderBrowser.tsx";
import { slug } from "../setup/SetupProject.tsx";

/** How long the page keeps trying to reach the team while its notes are copied: ten minutes. */
const MOVE_FOR_MS = 10 * 60_000;

/** What holds the move, as `project.convert_blockers` answers it. */
type Blockers = {
	tasks: string[];
	catervafication: boolean;
	folderChanges: number[];
};

/** The last part of a folder's path. */
function lastPart(path: string): string {
	return path.split("/").filter(Boolean).at(-1) ?? "";
}

/**
 * The name a repository is given in a project: its folder's name in lowercase letters and digits
 * in words joined by single dashes, starting with a letter, at most 32 characters.
 */
export function repoNameOf(folder: string): string {
	const words = folder
		.normalize("NFKD")
		.toLowerCase()
		.replace(/[^a-z0-9]+/g, "-")
		.replace(/^-+|-+$/g, "");
	const named = /^[a-z]/.test(words) ? words : `repo-${words}`;
	return named.slice(0, 32).replace(/-+$/g, "") || "repo";
}

/** What the team is working on, in words: `CTV-1, CTV-2 and 2 changes to your team’s notes`. */
function work(blockers: Blockers): string {
	const items = [
		...blockers.tasks,
		...(blockers.catervafication ? [t("reposAddCatervafication")] : []),
		...(blockers.folderChanges.length === 0
			? []
			: [
					blockers.folderChanges.length === 1
						? t("reposAddChangesOne")
						: t("reposAddChangesMany", {
								count: String(blockers.folderChanges.length),
							}),
				]),
	];
	const last = items.at(-1);
	return items.length > 1
		? `${items.slice(0, -1).join(", ")} and ${last}`
		: (last ?? "");
}

/** What the daemon's own sentence for a held move holds, to fill the owner's words with. */
function fillOf(
	refusal: { message: string },
	repo: string,
): Record<string, string> {
	const held = /working in (.+?): (.+)\. Let them finish/.exec(refusal.message);
	return { repo: held?.[1] ?? repo, work: held?.[2] ?? "" };
}

/**
 * Settings, Repositories, in a project of one repository: the dialog that adds a second one. It says
 * in words what changes, waits while the team has work in progress, asks for the second repository
 * and moves the team into a project folder (phase 9 step 07b). `second` is a repository already
 * chosen, such as the one a "Your teams" card holds.
 */
export function AddSecondRepository(props: {
	/** The project's folder, as `serve.status` gives it. */
	root: string;
	/** A second repository chosen already: its folder relative to home, and the name to give it. */
	second?: { path: string; name?: string };
	onClose: () => void;
	/** Called once the move is under way, with the project folder as `~/…`. */
	onMoved: (folder: string) => void;
}) {
	const { client, reopen } = useConnection();
	const { data: blockers } = useQuery<Blockers>("project.convert_blockers", {});
	const folder = lastPart(props.root);
	const [firstName, setFirstName] = useState(repoNameOf(folder));
	const [parent, setParent] = useState("Catervas");
	const [choosingPlace, setChoosingPlace] = useState(false);
	const name = slug(folder) || "project";
	const [stage, setStage] = useState<"explain" | "second">("explain");
	const [picked, setPicked] = useState(props.second?.path);
	const [secondName, setSecondName] = useState(
		props.second?.name ?? repoNameOf(lastPart(props.second?.path ?? "")),
	);
	const [branch, setBranch] = useState("");
	const [choosing, setChoosing] = useState(props.second === undefined);
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const held = blockers ? work(blockers) : undefined;
	const holds = blockers !== undefined && held !== "";
	const place = `~/${parent}/${name}`;

	const move = async () => {
		if (!client || picked === undefined) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("project.convert", {
				firstName,
				parent,
				name,
				second: {
					name: secondName,
					path: picked,
					...(branch.trim() !== "" && { integrationBranch: branch.trim() }),
				},
			});
			props.onMoved(place);
			reopen(MOVE_FOR_MS);
		} catch (e) {
			const words = refusalsOf(e).map((one) =>
				said(one.code, fillOf(one, folder), "reposFailed"),
			);
			setRefused([...new Set(words)].join(" "));
			setBusy(false);
		}
	};

	return (
		<Dialog
			open
			wide
			title={t("reposAddTitle", { repo: folder })}
			onClose={props.onClose}
			actions={
				<>
					<Button onClick={props.onClose}>{t("agentCancel")}</Button>
					{stage === "explain" ? (
						<Button
							kind="primary"
							disabled={blockers === undefined || holds || firstName === ""}
							onClick={() => setStage("second")}
						>
							{t("reposContinue")}
						</Button>
					) : (
						<Button
							kind="primary"
							busy={busy}
							disabled={picked === undefined || secondName === ""}
							onClick={move}
						>
							{t("reposContinue")}
						</Button>
					)}
				</>
			}
		>
			<div className={styles.form}>
				{stage === "explain" ? (
					<>
						<p>{t("reposAddLead", { repo: folder })}</p>
						<p>
							{t("reposAddMoves", { repo: folder })} <code>{place}</code>{" "}
							<Button
								kind="quiet"
								onClick={() => setChoosingPlace((on) => !on)}
							>
								{t("reposAddChange")}
							</Button>
						</p>
						{choosingPlace && (
							<FolderBrowser
								mode="parent"
								busy={false}
								onUse={(path) => {
									setParent(path);
									setChoosingPlace(false);
								}}
							/>
						)}
						<p>{t("reposAddBranch", { repo: folder })}</p>
						<p>{t("reposAddNothing", { repo: folder })}</p>
						{holds && (
							<p role="alert">
								{t("reposAddBusy", { repo: folder, work: held ?? "" })}
							</p>
						)}
						<TextField
							id="add-repo-first"
							label={t("reposAddNameFirst", { repo: folder })}
							hint={t("reposAddNext")}
							value={firstName}
							onChange={setFirstName}
						/>
					</>
				) : (
					<>
						<p>{t("reposAddPickTitle")}</p>
						{picked !== undefined && (
							<p>
								{t("reposAddPicked", { path: `~/${picked}` })}{" "}
								{!choosing && (
									<Button kind="quiet" onClick={() => setChoosing(true)}>
										{t("reposAddChange")}
									</Button>
								)}
							</p>
						)}
						{choosing && (
							<FolderBrowser
								mode="project"
								busy={false}
								onUse={(path) => {
									setPicked(path);
									setSecondName(repoNameOf(lastPart(path)));
									setChoosing(false);
								}}
							/>
						)}
						{picked !== undefined && (
							<>
								<TextField
									id="add-repo-second"
									label={t("reposAddSecondName")}
									value={secondName}
									onChange={setSecondName}
								/>
								<TextField
									id="add-repo-branch"
									label={t("reposAddSecondBranch")}
									value={branch}
									onChange={setBranch}
								/>
							</>
						)}
					</>
				)}
				{refused && <p role="alert">{refused}</p>}
			</div>
		</Dialog>
	);
}
