import type { RpcError } from "@farik/protocol-client";
import { AVATAR_URLS, type AvatarKey, Button } from "@farik/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { refusalsOf, said } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import { type Agent, type Judgment, ringOf } from "./setup/TeamSetup.tsx";

/** A saved team as `templates.list` answers it (spec 4.4). */
export type Template = {
	name: string;
	savedAt: string;
	agents: (Pick<Agent, "id" | "displayName" | "role"> &
		Partial<Pick<Agent, "persona" | "avatar" | "model">>)[];
	policy: {
		permissions: { runCommands: boolean; push: boolean };
		judgment: Judgment;
		integration: string;
	};
	budgets: { dailyUsd?: number };
};
export type Listing = {
	folder: string;
	templates: { slug: string; template: Template }[];
	unreadable: { slug: string }[];
};

/** The saved teams, or why there are none to read: Farik has no state folder. */
export function useTemplates() {
	const { data, error, again } = useQuery<Listing>("templates.list", {});
	return { listing: data, noFolder: noFolder(error), again };
}

/** Whether a refusal says Farik has no folder to keep saved teams in. */
export function noFolder(error: RpcError | undefined): boolean {
	return !!error && refusalsOf(error).some((e) => e.code === "no_state_folder");
}

/** "Mira", "Mira and Theo", "Mira, Ada and Theo". */
export function andList(names: string[]): string {
	const last = names.at(-1) ?? "";
	return names.length > 1
		? `${names.slice(0, -1).join(", ")} and ${last}`
		: last;
}

/** "Mira, Ada and Theo · saved 28 September", with the year where `year` asks for it. */
export function templateMeta(template: Template, year = false): string {
	const date = new Date(template.savedAt).toLocaleDateString("en-GB", {
		day: "numeric",
		month: "long",
		...(year && { year: "numeric" }),
	});
	return t("templateMeta", {
		names: andList(template.agents.map((a) => a.displayName)),
		date,
	});
}

/** A row of small faces, each in its role's ring; pictures only, the names are said beside them. */
export function Faces({ agents }: { agents: Template["agents"] }) {
	return (
		<span className={styles.faces}>
			{agents.map((a) => {
				const src = AVATAR_URLS[a.avatar as AvatarKey];
				return src ? (
					<img key={a.id} src={src} alt="" style={ringOf(a.role)} />
				) : null;
			})}
		</span>
	);
}

/** Settings' "Saved teams": each saved team with Rename and Delete, and a file Farik cannot read. */
export function SavedTeams() {
	const { listing, noFolder, again } = useTemplates();
	return (
		<section className={styles.section} aria-labelledby="saved-heading">
			<h2 id="saved-heading">{t("savedTeams")}</h2>
			{noFolder ? (
				<p className={styles.notice}>{t("templateNoFolder")}</p>
			) : listing && !listing.templates.length && !listing.unreadable.length ? (
				<p>
					{t("savedTeamsNoneBefore")}
					<Link to="/team">{t("team")}</Link>
					{t("savedTeamsNoneAfter")}
				</p>
			) : (
				listing && (
					<>
						<p>{t("savedTeamsLead")}</p>
						<ul className={styles.savedRows}>
							{listing.templates.map(({ slug, template }) => (
								<SavedRow
									key={slug}
									slug={slug}
									template={template}
									onChanged={again}
								/>
							))}
							{listing.unreadable.map(({ slug }) => (
								<SavedRow key={slug} slug={slug} onChanged={again} />
							))}
						</ul>
						<p className={styles.muted}>
							{t("savedFolder", { folder: listing.folder })}
						</p>
					</>
				)
			)}
		</section>
	);
}

/** One saved team, or a file Farik cannot read (no `template`), which offers Delete alone. */
function SavedRow({
	slug,
	template,
	onChanged,
}: {
	slug: string;
	template?: Template;
	onChanged: () => void;
}) {
	const { client } = useConnection();
	const name = template?.name ?? slug;
	const [doing, setDoing] = useState<"rename" | "delete">();
	const [next, setNext] = useState(name);
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const send = async (method: "template.rename" | "template.delete") => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call(
				method,
				method === "template.rename" ? { slug, name: next } : { slug },
			);
			setDoing(undefined);
			onChanged();
		} catch (e) {
			const code = refusalsOf(e)[0]?.code;
			// Renaming onto another's name has no Replace: it says to choose another.
			setRefused(
				code === "template_exists"
					? t("templateRenameTaken", { name: next.trim() })
					: said(code),
			);
		}
		setBusy(false);
	};
	const id = `rename-${slug}`;
	return (
		<li>
			<div>
				{doing === "rename" ? (
					<>
						<label htmlFor={id}>
							<strong>{t("savedRenameLabel", { name })}</strong>
						</label>
						<span className={styles.actions}>
							<input
								id={id}
								className={styles.select}
								value={next}
								onChange={(e) => setNext(e.target.value)}
							/>
							<Button
								kind="primary"
								busy={busy}
								onClick={() => send("template.rename")}
							>
								{t("savedRenameSave")}
							</Button>
							<Button onClick={() => setDoing(undefined)}>
								{t("agentCancel")}
							</Button>
						</span>
					</>
				) : template ? (
					<strong>{name}</strong>
				) : (
					<code>{slug}</code>
				)}
				{template ? (
					<span className={styles.cardHead}>
						<Faces agents={template.agents} />
						<span className={styles.muted}>{templateMeta(template, true)}</span>
					</span>
				) : (
					<strong>{t("templateUnreadable")}</strong>
				)}
			</div>
			{!doing && (
				<span className={styles.actions}>
					{template && (
						<Button
							onClick={() => {
								setNext(name);
								setDoing("rename");
							}}
						>
							{t("savedRename")} <span className={styles.hidden}>{name}</span>
						</Button>
					)}
					<Button onClick={() => setDoing("delete")}>
						{t("savedDelete")} <span className={styles.hidden}>{name}</span>
					</Button>
				</span>
			)}
			{doing === "delete" && (
				<div className={styles.confirm}>
					<p>
						<strong>{t("savedDeleteAsk", { name })}</strong>{" "}
						{t("savedDeleteNote")}
					</p>
					<span className={styles.actions}>
						<Button busy={busy} onClick={() => send("template.delete")}>
							{t("savedDeleteYes")}
						</Button>
						<Button onClick={() => setDoing(undefined)}>
							{t("savedDeleteNo")}
						</Button>
					</span>
				</div>
			)}
			{refused && (
				<p role="alert" className={styles.alert}>
					{refused}
				</p>
			)}
		</li>
	);
}
