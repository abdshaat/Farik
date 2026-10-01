import {
	AVATAR_URLS,
	type AvatarKey,
	Button,
	Choice,
	Dialog,
	RoleTag,
} from "@farik/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { type Refusal, said, saidAll } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import styles from "../pages.module.css";
import { andList, Faces, templateMeta, useTemplates } from "../SavedTeams.tsx";
import {
	type Agent,
	ringOf,
	roleName,
	type Team,
} from "../setup/TeamSetup.tsx";

/** `template.preview`'s answer, and `template.apply`'s: the team as it would be, and who moves. */
type Preview = {
	team: Team;
	kept: string[];
	retired: string[];
	removed: string[];
	added: string[];
	effects: string[];
	errors: Refusal[];
	/** The template file's digest as this preview read it, which `template.apply` is given. */
	digest: string;
};

/** The role a missing-role refusal names. */
const MISSING: Record<string, Agent["role"]> = {
	needs_developer: "software_developer",
	needs_product_manager: "product_manager",
};

/**
 * Why a saved team cannot be used here, in the words written for templates: who of the missing
 * role stays paused and who would be retired, and what to do first.
 */
function refusedLine(error: Refusal, preview: Preview): string {
	const role = MISSING[error.code ?? ""];
	if (!role) return `${said(error.code)} ${t("templateChooseAnother")}`;
	const named = (keep: (a: Agent) => boolean) =>
		andList(
			preview.team.agents
				.filter((a) => a.role === role && keep(a))
				.map((a) => a.displayName),
		);
	const paused = named((a) => a.status === "paused");
	const retired = named((a) => preview.retired.includes(a.id));
	const line = t("templateNoActive", { role: roleName(role) });
	if (!paused) return `${line} ${t("templateChooseAnother")}`;
	return [
		line,
		retired
			? t("templatePausedRetired", { paused, retired })
			: t("templatePaused", { paused }),
		t("templateResume", { paused }),
	].join(" ");
}

/**
 * "Use a saved team": pick one, see what changes, then `template.apply`. Once it is applied,
 * `onApplied` reads the team again at once, so nothing on the page acts on the team it replaced.
 */
export function UseTemplate({
	current,
	onApplied,
	onClose,
}: {
	current: Team;
	onApplied: () => void;
	onClose: () => void;
}) {
	const { client } = useConnection();
	const { listing, noFolder } = useTemplates();
	const saved = listing?.templates ?? [];
	const [picked, setPicked] = useState<string>();
	const slug = picked ?? saved[0]?.slug;
	const [shown, setShown] = useState<string>();
	const { data: preview } = useQuery<Preview>(
		"template.preview",
		{ slug: shown },
		!shown,
	);
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const template = saved.find((s) => s.slug === shown)?.template;
	const name = template?.name ?? "";

	const apply = async () => {
		if (!client || !shown || !preview) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("template.apply", {
				slug: shown,
				digest: preview.digest,
			});
			onApplied();
			onClose();
		} catch (e) {
			setRefused(saidAll(e));
			setBusy(false);
		}
	};

	if (shown && preview) {
		const of = (ids: string[], team: Team) =>
			ids.flatMap((id) => team.agents.filter((a) => a.id === id));
		const groups = [
			["useStays", "useStaysNote", of(preview.kept, preview.team)],
			["useJoins", "useJoinsNote", of(preview.added, preview.team)],
			["useRetired", "useRetiredNote", of(preview.retired, preview.team)],
			["useRemoved", "useRemovedNote", of(preview.removed, current)],
		] as const;
		const lines = [
			...new Set(preview.errors.map((e) => refusedLine(e, preview))),
		];
		return (
			<Dialog
				open
				title={t("usePreviewTitle", { name })}
				onClose={onClose}
				actions={
					<>
						<Button onClick={() => setShown(undefined)}>{t("back")}</Button>
						<Button
							kind="primary"
							busy={busy}
							disabled={lines.length > 0}
							onClick={apply}
						>
							{t("useApply")}
						</Button>
					</>
				}
			>
				{lines.length > 0 ? (
					<div role="alert" className={styles.alert}>
						<strong>{t("useCannot")}</strong>
						{lines.map((line) => (
							<p key={line}>{line}</p>
						))}
						<p className={styles.muted}>{t("useNothingChanged")}</p>
					</div>
				) : (
					<p>{t("usePreviewLead")}</p>
				)}
				<div className={styles.groups}>
					{groups.map(([title, note, agents]) => (
						<section key={title} aria-labelledby={`group-${title}`}>
							<h3 id={`group-${title}`} className={styles.groupTitle}>
								{t(title)} <span className={styles.muted}>{agents.length}</span>
							</h3>
							<p className={styles.muted}>{t(note)}</p>
							{agents.length ? (
								<ul className={styles.people}>
									{agents.map((agent) => (
										<Person key={agent.id} agent={agent} />
									))}
								</ul>
							) : (
								<p className={styles.muted}>{t("useNobody")}</p>
							)}
						</section>
					))}
				</div>
				{preview.effects.length > 0 && (
					<section className={styles.changes} aria-labelledby="use-changes">
						<h3 id="use-changes" className={styles.groupTitle}>
							{t("useChanges")}
						</h3>
						<ul>
							{preview.effects.map((effect) => (
								<li key={effect}>{effect}</li>
							))}
						</ul>
					</section>
				)}
				{refused && (
					<p role="alert" className={styles.alert}>
						{refused}
					</p>
				)}
			</Dialog>
		);
	}

	return (
		<Dialog
			open
			title={t("templateUseOpen")}
			onClose={onClose}
			actions={
				<>
					<Button onClick={onClose}>{t("agentCancel")}</Button>
					<Button
						kind="primary"
						disabled={!slug}
						onClick={() => setShown(slug)}
					>
						{t("useShow")}
					</Button>
				</>
			}
		>
			<p>{t("useLead")}</p>
			{saved.length > 0 ? (
				<Choice<string>
					name="use-saved"
					legend={t("useList")}
					value={slug ?? ""}
					onChange={setPicked}
					options={saved.map(({ slug, template }) => ({
						value: slug,
						label: template.name,
						description: templateMeta(template),
						extra: <Faces agents={template.agents} />,
					}))}
				/>
			) : (
				(listing || noFolder) && (
					<p>{t(noFolder ? "templateNoFolder" : "startSavedNone")}</p>
				)
			)}
			<p className={styles.muted}>
				{t("useSettingsBefore")}
				<Link to="/settings">{t("settings")}</Link>.
			</p>
		</Dialog>
	);
}

/** One agent in a preview group: face, name, role tag, and the role in words. */
function Person({ agent }: { agent: Agent }) {
	const avatar = AVATAR_URLS[agent.avatar as AvatarKey];
	const role = roleName(agent.role);
	return (
		<li className={styles.cardHead}>
			{avatar && (
				<img
					className={styles.smallFace}
					style={ringOf(agent.role)}
					src={avatar}
					alt=""
				/>
			)}
			<span>
				<strong>{agent.displayName}</strong> <RoleTag role={agent.role} />
				<br />
				<span className={styles.muted}>
					{agent.status === "paused" ? t("usePaused", { role }) : role}
				</span>
			</span>
		</li>
	);
}
