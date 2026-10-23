import { Button, Dialog, TextField } from "@catervas/ui";
import { useState } from "react";
import { useHub } from "../../app/connection.tsx";
import { t } from "../../strings/t.ts";
import { FolderBrowser } from "../setup/FolderBrowser.tsx";
import styles from "./find.module.css";
import { findingSaid } from "./finding.ts";

/** A project typed in: its name, and a folder under home, a repository's address, or both. */
export function AddProjectDialog(props: {
	onClose: () => void;
	/** Called once the hub kept the project. */
	onAdded: () => void;
}) {
	const { client } = useHub();
	const [name, setName] = useState("");
	const [repository, setRepository] = useState("");
	const [folder, setFolder] = useState<string>();
	const [choosing, setChoosing] = useState(false);
	const [busy, setBusy] = useState(false);
	const [said, setSaid] = useState<string>();
	const add = async () => {
		if (!client) return;
		setBusy(true);
		setSaid(undefined);
		try {
			await client.call("finding.add_project", {
				name: name.trim(),
				...(folder !== undefined && { folder }),
				...(repository.trim() !== "" && { repository: repository.trim() }),
			});
			props.onAdded();
		} catch (e) {
			setSaid(findingSaid(e, "addProjectFailed"));
		} finally {
			setBusy(false);
		}
	};
	return (
		<Dialog
			open
			title={t("addProjectTitle")}
			onClose={props.onClose}
			actions={
				<>
					<Button onClick={props.onClose}>{t("agentCancel")}</Button>
					<Button
						kind="primary"
						busy={busy}
						disabled={
							name.trim() === "" ||
							(folder === undefined && repository.trim() === "")
						}
						onClick={add}
					>
						{t("addProjectAdd")}
					</Button>
				</>
			}
		>
			<div className={styles.form}>
				<TextField
					id="add-project-name"
					label={t("addProjectName")}
					value={name}
					onChange={setName}
				/>
				<div>
					<p>
						{t("addProjectFolder")}
						{folder !== undefined && <> · {`~/${folder}`}</>}
					</p>
					<Button onClick={() => setChoosing((on) => !on)}>
						{folder === undefined
							? t("addProjectChoose")
							: t("addProjectChanged")}
					</Button>
					{choosing && (
						<FolderBrowser
							mode="parent"
							busy={false}
							onUse={(path) => {
								setFolder(path);
								setChoosing(false);
							}}
						/>
					)}
				</div>
				<TextField
					id="add-project-repository"
					label={t("addProjectRepository")}
					hint={t("addProjectRepositoryHint")}
					value={repository}
					onChange={setRepository}
				/>
				{said && <p role="alert">{said}</p>}
			</div>
		</Dialog>
	);
}
