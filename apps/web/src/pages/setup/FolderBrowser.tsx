import { Button } from "@farik/ui";
import { useState } from "react";
import { daemonSaid } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";

type Listing = {
	path: string;
	parent: string | null;
	entries: { name: string; git: boolean }[];
};

/** `name` inside `path`, both relative to home. */
export const join = (path: string, name: string) =>
	path ? `${path}/${name}` : name;

/** The folders in the user's home, one folder at a time; paths are relative to home. */
export function FolderBrowser(props: {
	/** A project must be a git project; a new project's parent can be any folder. */
	mode: "project" | "parent";
	busy: boolean;
	onUse: (path: string) => void;
}) {
	const [path, setPath] = useState("");
	const [selected, setSelected] = useState<string>();
	const { data, error } = useQuery<Listing>("folders.list", { path });
	const go = (to: string) => {
		setPath(to);
		setSelected(undefined);
	};
	const entry = data?.entries.find((e) => e.name === selected);
	const shown = data?.path ?? path;
	const target = entry ? join(shown, entry.name) : shown;
	const usable = props.mode === "parent" || entry?.git === true;
	const parts = shown ? shown.split("/") : [];

	return (
		<fieldset className={styles.browser}>
			<legend className={styles.hidden}>{t("folders")}</legend>
			<p className={styles.crumb}>
				<button type="button" onClick={() => go("")}>
					{t("home")}
				</button>
				{" /"}
				{parts.map((part, i) => (
					<span key={parts.slice(0, i + 1).join("/")}>
						<button
							type="button"
							onClick={() => go(parts.slice(0, i + 1).join("/"))}
						>
							{part}
						</button>
						{" /"}
					</span>
				))}
			</p>
			{error && (
				<p role="alert" className={styles.alert}>
					{daemonSaid(error, "setupUnreadable")}
				</p>
			)}
			<ul className={styles.items}>
				{data && data.parent !== null && (
					<li>
						<button
							type="button"
							className={styles.item}
							onClick={() => go(data.parent ?? "")}
						>
							{t("upOne")}
						</button>
					</li>
				)}
				{data?.entries.map((e) => (
					<li key={e.name}>
						<button
							type="button"
							className={styles.item}
							aria-pressed={e.name === selected}
							onClick={() => setSelected(e.name)}
							onDoubleClick={() => go(join(shown, e.name))}
						>
							<span>{e.name}</span>
							<small>{t(e.git ? "gitProject" : "notGitProject")}</small>
						</button>
					</li>
				))}
				{data?.entries.length === 0 && (
					<li className={styles.item}>{t("noFolders")}</li>
				)}
			</ul>
			<div className={styles.act}>
				<span>
					{t("selected")} <span className={styles.path}>{`~/${target}`}</span>
				</span>
				<span>
					<Button disabled={!entry} onClick={() => go(target)}>
						{t("openFolder")}
					</Button>
					<Button
						kind="primary"
						busy={props.busy}
						disabled={!usable}
						onClick={() => props.onUse(target)}
					>
						{t("useFolder")}
					</Button>
				</span>
			</div>
		</fieldset>
	);
}
