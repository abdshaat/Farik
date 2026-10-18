import {
	AVATAR_URLS,
	type AvatarKey,
	Button,
	InfoTip,
	type Role,
} from "@catervas/ui";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { type Refusal, refusalsOf, said } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import { Bullets } from "../../components/Bullets.tsx";
import {
	effortLine,
	type ModelOption,
	modelName,
} from "../../components/ModelFields.tsx";
import { t } from "../../strings/t.ts";
import pages from "../pages.module.css";
import { type Template, useTemplates } from "../SavedTeams.tsx";
import { AgentSheet, JOB_ROLES } from "./AgentSheet.tsx";
import { PreviewFields } from "./PreviewFields.tsx";
import setup from "./setup.module.css";
import styles from "./TeamPicker.module.css";
import {
	agentOfRole,
	type Draft,
	draftOf,
	ringOf,
	roleName,
	type SuggestedTeam,
	teamOf,
	useSetup,
} from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

type Checked = { errors: Refusal[] };
type Member = Draft["members"][number];

/** Each ready-made team's line, by its slug; a slug Catervas does not know shows its names alone. */
const BUILTIN_LINES = {
	"software-product": "builtinSoftwareProduct",
	"online-store": "builtinOnlineStore",
	"financial-analysis": "builtinFinancialAnalysis",
} as const;
/** The most people a team may hold. */
const MOST = 7;

/** A way to start, as the cards name it. */
type Way =
	| { kind: "builtin" | "saved"; slug: string; template: Template }
	| { kind: "solo" | "scratch" };

/** Which card the draft stands for. */
function pickOf(draft: Draft, cards: Way[]): string {
	if (draft.start === "solo" || draft.start === "scratch") return draft.start;
	const found = cards.find(
		(c) =>
			(c.kind === "builtin" || c.kind === "saved") &&
			c.kind === draft.start &&
			c.template.name === draft.from,
	);
	return found ? keyOf(found) : "";
}

function keyOf(card: Way): string {
	return card.kind === "builtin" || card.kind === "saved"
		? `${card.kind}:${card.slug}`
		: card.kind;
}

/** The one line under the title: why the first card is first. */
function pickedLine(top: SuggestedTeam | undefined): string {
	if (!top) return t("pickedNone");
	if (top.signal === "money") return t("pickedMoney");
	if (top.signal === "shop") return t("pickedShop");
	if (top.signal === "new") return t("pickedNew");
	if (!top.language) return t("pickedCodeAny");
	return t(top.tests ? "pickedCodeTests" : "pickedCode", {
		language: top.language,
	});
}

/** The job one agent alone takes by default, from what the folder holds most of. */
function soloRoleFor(top: SuggestedTeam | undefined): Role {
	if (top?.signal === "money") return "finance_specialist";
	if (top?.signal === "shop") return "marketing_specialist";
	return "software_developer";
}

/** A row of faces, each in its role's ring. */
function Faces({
	agents,
	size,
}: {
	agents: { role: Role; avatar?: string | undefined }[];
	size?: "large";
}) {
	return (
		<span className={styles.faces} data-size={size}>
			{agents.map((a, i) => {
				const src = AVATAR_URLS[a.avatar as AvatarKey];
				return src ? (
					// biome-ignore lint/suspicious/noArrayIndexKey: two agents may share a picture
					<img key={i} src={src} alt="" style={ringOf(a.role)} />
				) : null;
			})}
		</span>
	);
}

/** Setup's fifth step: a card per way to start, the folder's team first, then who is on it. */
export function SetupTeam() {
	const { client } = useConnection();
	const navigate = useNavigate();
	const { draft, proposed, change, unavailable, checkAgain } = useSetup();
	const { listing } = useTemplates();
	const { data: modelList } = useQuery<{ models: ModelOption[] }>(
		"models.list",
		{},
	);
	const models = modelList?.models ?? [];
	const order = proposed.suggestedTeams ?? [];
	const rank = (slug: string) => {
		const at = order.findIndex((s) => s.slug === slug);
		return at < 0 ? order.length : at;
	};
	const builtin = [...(listing?.builtin ?? [])].sort(
		(a, b) => rank(a.slug) - rank(b.slug),
	);
	const top = order[0];
	const soloRole = soloRoleFor(top);
	const cards: Way[] = [
		...builtin.map((b) => ({ kind: "builtin" as const, ...b })),
		{ kind: "solo" },
		...(listing?.templates ?? []).map((s) => ({
			kind: "saved" as const,
			...s,
		})),
		{ kind: "scratch" },
	];
	const [hero, ...rest] = cards;
	const [errors, setErrors] = useState<Refusal[]>([]);
	const [busy, setBusy] = useState(false);
	const [open, setOpen] = useState<number>();
	const [editingPreview, setEditingPreview] = useState(
		() => !draft.preview.start.trim(),
	);

	const choose = (card: Way) => {
		setErrors([]);
		change(
			card.kind === "builtin" || card.kind === "saved"
				? draftOf(proposed, card.kind, card.template)
				: draftOf(proposed, card.kind, undefined, soloRole),
		);
	};
	// A first visit starts on the card the folder fits best, not on the old suggested six.
	const first = builtin[0];
	useEffect(() => {
		if (draft.start === "suggested" && first)
			change(draftOf(proposed, "builtin", first.template));
	}, [draft.start, first, proposed, change]);

	const solo = draft.start === "solo";
	const picked = pickOf(draft, cards);
	const shown = draft.members
		.map((member, index) => ({ member, index }))
		.filter(({ member }) => !member.more || member.on || member.added);
	const on = draft.members.filter((m) => m.on);
	const unnamed = on.some((m) => !m.agent.displayName.trim());
	const at = (member: Member) =>
		member.on
			? errors.filter(
					(e) =>
						e.path.split("/").slice(1, 3).join("/") ===
						`agents/${on.indexOf(member)}`,
				)
			: [];
	const loose = errors.filter(
		(e) => !/^\/agents\/\d+/.test(e.path) && !e.path.startsWith("/preview"),
	);

	const update = (index: number, next: Partial<Member>) => {
		setErrors([]);
		change({
			...draft,
			members: draft.members.map((m, i) =>
				i === index ? { ...m, ...next } : m,
			),
		});
	};
	const add = () => {
		setErrors([]);
		const agent = agentOfRole(
			proposed,
			"software_developer",
			draft.members.map((m) => m.agent),
		);
		const key = Math.max(-1, ...draft.members.map((m) => m.key)) + 1;
		change({
			...draft,
			members: [
				...draft.members,
				{ agent, on: true, key, more: true, added: true },
			],
		});
		setOpen(draft.members.length);
	};
	const onward = async () => {
		if (!client) return;
		setBusy(true);
		try {
			const checked = (await client.query("team.validate", {
				team: teamOf(draft),
			})) as Checked;
			setErrors(checked.errors);
			// A saved team's permission answers carry over, so they are not asked again.
			if (checked.errors.length === 0)
				navigate(draft.from ? "/setup/spending" : "/setup/permissions");
		} catch (e) {
			setErrors(refusalsOf(e));
		}
		setBusy(false);
	};

	const card = (c: Way, big = false) => {
		const key = keyOf(c);
		const chosen = picked === key;
		const suggested = big && c.kind === "builtin" && order.length > 0;
		const { name, line, agents } =
			c.kind === "builtin" || c.kind === "saved"
				? {
						name: c.template.name,
						line:
							c.kind === "builtin" && c.slug in BUILTIN_LINES
								? t(BUILTIN_LINES[c.slug as keyof typeof BUILTIN_LINES])
								: t("pickSavedLine"),
						agents: c.template.agents.map((a) => ({
							role: a.role,
							avatar: a.avatar ?? agentOfRole(proposed, a.role, []).avatar,
						})),
					}
				: c.kind === "solo"
					? {
							name: t("pickSolo"),
							line: t("pickSoloLine"),
							agents: [agentOfRole(proposed, soloRole, [])],
						}
					: {
							name: t("pickScratch"),
							line: t("pickScratchLine"),
							agents: [agentOfRole(proposed, "product_manager", [])],
						};
		return (
			<label
				key={key}
				className={styles.card}
				data-big={big || undefined}
				data-chosen={chosen || undefined}
			>
				<input
					type="radio"
					className={styles.radio}
					name="start"
					value={key}
					checked={chosen}
					onChange={() => choose(c)}
				/>
				{chosen && (
					<span className={styles.check} aria-hidden="true">
						✓
					</span>
				)}
				<span className={styles.facesRow}>
					<Faces agents={agents} {...(big && { size: "large" as const })} />
					{c.kind === "scratch" && (
						<span className={styles.plus} aria-hidden="true">
							+
						</span>
					)}
				</span>
				<span className={styles.cardText}>
					<span className={styles.cardName}>
						{name}
						{suggested && (
							<span className={styles.badge}>{t("pickSuggested")}</span>
						)}
					</span>
					<span className={styles.cardLine}>{line}</span>
				</span>
			</label>
		);
	};

	const row = ({ member, index }: { member: Member; index: number }) => {
		const { agent } = member;
		const avatar = AVATAR_URLS[agent.avatar as AvatarKey];
		const cannot = unavailable.includes(agent.role);
		const why = at(member);
		const shipped = proposed.roleDefaults?.[agent.role];
		const modelId = agent.model?.id ?? shipped?.model;
		const effort = agent.model?.effort ?? shipped?.effort;
		const name = agent.displayName.trim();
		return (
			<li key={member.key}>
				<button
					type="button"
					className={styles.row}
					data-off={!member.on || undefined}
					aria-label={t("whoEdit", { name: name || roleName(agent.role) })}
					onClick={() => setOpen(index)}
				>
					{avatar && (
						<img
							className={styles.rowFace}
							style={ringOf(agent.role)}
							src={avatar}
							alt=""
						/>
					)}
					<span className={styles.rowWho}>
						<span className={styles.rowName}>
							{name || <em>{t("whoNeedsName")}</em>}
						</span>
						<span className={styles.rowRole}>{roleName(agent.role)}</span>
					</span>
					<span className={styles.rowModel}>
						{cannot ? (
							<span>{t("whoNeedsDocker")}</span>
						) : !member.on ? (
							<span>{t("whoOff")}</span>
						) : (
							<>
								<span className={styles.rowModelName}>
									{modelName(modelId, models)}
								</span>
								<span>{effortLine(effort)}</span>
							</>
						)}
					</span>
					<span className={styles.chevron} aria-hidden="true">
						›
					</span>
				</button>
				{why.length > 0 && (
					<p className={styles.rowError}>
						{why.map((e) => said(e.code)).join(" ")}
					</p>
				)}
			</li>
		);
	};

	const sheet = () => {
		if (open === undefined) return null;
		const member = draft.members[open];
		if (!member) return null;
		const { agent } = member;
		const shipped = proposed.roleDefaults?.[agent.role];
		const cannot = unavailable.includes(agent.role);
		const designer = agent.role === "ui_ux_designer";
		const close = () => setOpen(undefined);
		const why = at(member);
		return (
			<AgentSheet
				agent={agent}
				index={open}
				on={member.on}
				models={models}
				roleModel={shipped ? { id: shipped.model, effort: shipped.effort } : {}}
				jobs={
					solo
						? JOB_ROLES.filter(
								(r) => r !== "scrum_master" && r !== "ui_ux_designer",
							)
						: member.more
							? JOB_ROLES.filter((r) => r !== "product_manager")
							: []
				}
				solo={solo}
				required={agent.role === "product_manager" && !member.more}
				error={why.map((e) => said(e.code)).join(" ") || undefined}
				onChange={(next) => update(open, { agent: next })}
				onRole={(role) =>
					update(open, {
						// A job this computer cannot run starts off the team, as a suggested row does.
						on: !unavailable.includes(role),
						agent: {
							...agentOfRole(
								proposed,
								role,
								draft.members.filter((_, i) => i !== open).map((m) => m.agent),
							),
							// A name the owner typed stays; a suggested one follows the job.
							...(agent.displayName.trim() &&
								!proposed.team.agents.some(
									(a) => a.displayName === agent.displayName,
								) && { displayName: agent.displayName }),
						},
					})
				}
				onToggle={(next) => update(open, { on: next })}
				onRemove={
					member.more && !solo
						? () => {
								close();
								change({
									...draft,
									members: draft.members.filter((_, i) => i !== open),
								});
							}
						: undefined
				}
				onClose={close}
			>
				{cannot && (
					<section className={styles.notice}>
						<h3>{t("teamNeedsSandbox")}</h3>
						<p>{t("teamNeedsSandboxNote", { designer: agent.displayName })}</p>
						<div className={setup.buttons}>
							<Button onClick={checkAgain}>{t("teamCheckAgain")}</Button>
							<a
								href="https://docs.docker.com/get-started/get-docker/"
								target="_blank"
								rel="noreferrer"
							>
								{t("teamInstallDocker")}
							</a>
						</div>
					</section>
				)}
				{designer && member.on && !cannot && (
					<PreviewSection
						draft={draft}
						index={open}
						designer={agent.displayName}
						errors={errors}
						editing={editingPreview}
						onEdit={() => setEditingPreview(true)}
						onChange={(preview) => {
							setErrors(errors.filter((e) => !e.path.startsWith("/preview")));
							change({ ...draft, preview });
						}}
					/>
				)}
			</AgentSheet>
		);
	};

	return (
		<Wizard
			step={4}
			title={t("teamTitle")}
			lead={pickedLine(top)}
			carried={!!draft.from}
		>
			<fieldset className={styles.cards}>
				<legend className={styles.hidden}>{t("pickStarts")}</legend>
				{hero && card(hero, true)}
				<div className={styles.grid}>{rest.map((c) => card(c))}</div>
			</fieldset>
			<section className={styles.who} aria-labelledby="who-title">
				<h2 id="who-title" className={styles.whoTitle}>
					{t(solo ? "whoSoloTitle" : "whoTitle")}
				</h2>
				<ul className={styles.list}>
					{shown.map(row)}
					{!solo && on.length < MOST && (
						<li>
							<button type="button" className={styles.addRow} onClick={add}>
								<span className={styles.addMark} aria-hidden="true">
									+
								</span>
								{t("teamAdd")}
							</button>
						</li>
					)}
				</ul>
			</section>
			{loose.length > 0 && (
				<p role="alert" className={setup.alert}>
					{loose.map((e) => said(e.code)).join(" ")}
				</p>
			)}
			<div className={setup.foot}>
				<Button onClick={() => navigate("/setup/scan")}>{t("back")}</Button>
				<span>
					{unnamed && <span className={setup.note}>{t("teamNameAll")}</span>}
					<Button
						kind="primary"
						busy={busy}
						disabled={on.length === 0 || unnamed}
						onClick={onward}
					>
						{solo && on[0]
							? t("soloContinue", { name: on[0].agent.displayName })
							: t("teamContinue")}
					</Button>
				</span>
			</div>
			{sheet()}
		</Wizard>
	);
}

/** How the Designer opens the app: what Catervas found, or the fields to say it. */
function PreviewSection({
	draft,
	index,
	designer,
	errors,
	editing,
	onEdit,
	onChange,
}: {
	draft: Draft;
	index: number;
	designer: string;
	errors: Refusal[];
	editing: boolean;
	onEdit: () => void;
	onChange: (preview: Draft["preview"]) => void;
}) {
	const id = `preview-${index}`;
	return (
		<section className={styles.notice} aria-labelledby={id}>
			{editing || errors.some((e) => e.path.startsWith("/preview")) ? (
				<>
					<div className={pages.titled}>
						<h3 id={id}>{t("previewSetupTitle", { designer })}</h3>
						<InfoTip id={`${id}-more-info`}>
							<Bullets text={t("previewMore")} />
						</InfoTip>
					</div>
					<div id={`${id}-fields`}>
						<PreviewFields
							id={id}
							form={draft.preview}
							designer={designer}
							errors={errors}
							onChange={onChange}
						/>
					</div>
				</>
			) : (
				<>
					<h3 id={id}>{t("previewTitle")}</h3>
					<p>
						{t("previewFound", { designer, start: draft.preview.start.trim() })}
					</p>
					<Button
						kind="quiet"
						expanded={false}
						controls={`${id}-fields`}
						onClick={() => {
							onEdit();
							requestAnimationFrame(() =>
								document.getElementById(`${id}-start`)?.focus(),
							);
						}}
					>
						{t("previewChange")}
					</Button>
				</>
			)}
		</section>
	);
}
