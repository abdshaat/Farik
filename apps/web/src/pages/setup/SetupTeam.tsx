import {
	AVATAR_URLS,
	type AvatarKey,
	Button,
	Choice,
	InfoTip,
} from "@catervas/ui";
import { useState } from "react";
import { useNavigate } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { type Refusal, refusalsOf, said } from "../../app/refusals.ts";
import { Bullets } from "../../components/Bullets.tsx";
import { t } from "../../strings/t.ts";
import pages from "../pages.module.css";
import {
	builtinMeta,
	Faces,
	templateMeta,
	useTemplates,
} from "../SavedTeams.tsx";
import { PreviewFields } from "./PreviewFields.tsx";
import styles from "./setup.module.css";
import {
	type Draft,
	draftOf,
	ringOf,
	roleName,
	type Start,
	someone,
	teamOf,
	useSetup,
} from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

type Checked = { errors: Refusal[] };

/** Each role's job, in the words of the approved setup mockups. */
const JOBS = {
	product_manager: "jobProductManager",
	scrum_master: "jobScrumMaster",
	architect: "jobArchitect",
	software_developer: "jobDeveloper",
	marketing_specialist: "jobMarketing",
	ui_ux_designer: "jobDesigner",
	finance_specialist: "jobFinance",
	procurement_specialist: "jobProcurement",
} as const;

/** Each ready-made team's line, by its slug; a slug Catervas does not know shows its names alone. */
const BUILTIN_LINES = {
	"software-product": "builtinSoftwareProduct",
	"online-store": "builtinOnlineStore",
	"financial-analysis": "builtinFinancialAnalysis",
} as const;

/** Setup's fifth step: where the team starts from, then its agents, named, and anyone added. */
export function SetupTeam() {
	const { client } = useConnection();
	const navigate = useNavigate();
	const { draft, proposed, change, unavailable, checkAgain } = useSetup();
	const { listing, noFolder } = useTemplates();
	const builtin = listing?.builtin ?? [];
	const saved = listing?.templates ?? [];
	// One chosen team per start, so switching starts never carries the other's choice.
	const [slugs, setSlugs] = useState<{ builtin?: string; saved?: string }>({});
	// The team a picker shows as chosen: the one picked here, else the one the rows already hold
	// (the draft outlives this screen, so Back comes back to it), else the first.
	const pick = (start: "builtin" | "saved") => {
		const list = start === "builtin" ? builtin : saved;
		return (
			slugs[start] ??
			(draft.start === start
				? list.find((l) => l.template.name === draft.from)?.slug
				: undefined) ??
			list[0]?.slug
		);
	};
	const begin = (start: Start, chosen?: string) => {
		setErrors([]);
		if (start === "builtin" || start === "saved") {
			const list = start === "builtin" ? builtin : saved;
			const slug = chosen ?? pick(start);
			setSlugs({ ...slugs, [start]: slug });
			change(
				draftOf(proposed, start, list.find((s) => s.slug === slug)?.template),
			);
		} else change(draftOf(proposed, start));
	};
	const scratch = draft.start === "scratch";
	const [errors, setErrors] = useState<Checked["errors"]>([]);
	const [busy, setBusy] = useState(false);
	const [editing, setEditing] = useState(() => !draft.preview.start.trim());
	const on = draft.members.filter((m) => m.on);
	// From scratch, the Product Manager and one more have to be ticked and named before going on;
	// a role offered under "More roles" is named by the daemon's check when it is ticked.
	const held =
		scratch &&
		(on.length < 2 ||
			draft.members.some(
				(m) => m.on && !m.more && !m.agent.displayName.trim(),
			));
	const pm =
		draft.members.find((m) => m.agent.role === "product_manager")?.agent
			.displayName ?? roleName("product_manager");
	const developer =
		draft.members.find((m) => m.agent.role === "software_developer")?.agent
			.displayName ?? roleName("software_developer");
	// An error's path names the agent by its place on the team, which leaves out the unticked.
	const at = (spot: number) =>
		errors.filter(
			(e) => e.path.split("/").slice(1, 3).join("/") === `agents/${spot}`,
		);
	const loose = errors.filter(
		(e) => !/^\/agents\/\d+/.test(e.path) && !e.path.startsWith("/preview"),
	);

	const set = (index: number, next: Partial<{ on: boolean; name: string }>) => {
		setErrors([]);
		change({
			...draft,
			members: draft.members.map((m, i) =>
				i !== index
					? m
					: {
							...m,
							on: next.on ?? m.on,
							agent: {
								...m.agent,
								displayName: next.name ?? m.agent.displayName,
							},
						},
			),
		});
	};
	const add = () => {
		setErrors([]);
		// A ready-made team may hold no Developer: the newcomer is like the suggested one.
		const developer = proposed.team.agents.find(
			(a) => a.role === "software_developer",
		);
		if (!developer) return;
		change({
			...draft,
			members: [
				...draft.members,
				{
					agent: someone(
						draft.members.map((m) => m.agent),
						developer,
					),
					on: true,
					// An id is handed out again once its name changes; a row's key is its own.
					key: draft.members.length,
					more: false,
				},
			],
		});
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

	/** One row: its tick, its picture in its role's ring, its role over its name, and its job. */
	const row = (member: Draft["members"][number], index: number) => {
		const { agent, on: included } = member;
		const role = roleName(agent.role);
		const avatar = AVATAR_URLS[agent.avatar as AvatarKey];
		const nameId = `name-${index}`;
		const why = included ? at(on.indexOf(member)) : [];
		const whyId = `why-${index}`;
		const designer = agent.role === "ui_ux_designer";
		const cannot = unavailable.includes(agent.role);
		return (
			<li key={member.key} className={styles.member}>
				{(member.more || !scratch || agent.role !== "product_manager") && (
					<input
						type="checkbox"
						checked={included}
						disabled={cannot}
						aria-label={t("teamInclude").replace("{role}", role)}
						onChange={(e) => set(index, { on: e.target.checked })}
					/>
				)}
				{avatar && (
					<img
						className={styles.face}
						style={ringOf(agent.role)}
						src={avatar}
						alt=""
					/>
				)}
				<span className={styles.who}>
					<label htmlFor={nameId}>{role}</label>
					<input
						id={nameId}
						className={styles.input}
						value={agent.displayName}
						aria-label={t("teamName").replace("{role}", role)}
						aria-invalid={why.length > 0 || undefined}
						aria-describedby={why.length > 0 ? whyId : undefined}
						onChange={(e) => set(index, { name: e.target.value })}
					/>
				</span>
				<span className={styles.persona}>
					{!member.more && scratch && agent.role === "product_manager"
						? t("teamScratchNote")
						: !member.more && draft.from
							? agent.persona
							: t(JOBS[agent.role], { pm, developer })}
				</span>
				{why.length > 0 && (
					<span id={whyId} className={styles.error}>
						{why.map((e) => said(e.code)).join(" ")}
					</span>
				)}
				{cannot && (
					<div className={styles.memberCard}>
						<strong>{t("teamNeedsSandbox")}</strong>
						<p>
							{t("teamNeedsSandboxNote", {
								designer: agent.displayName,
							})}
						</p>
						<div className={styles.buttons}>
							<Button onClick={checkAgain}>{t("teamCheckAgain")}</Button>
							<a
								href="https://docs.docker.com/get-started/get-docker/"
								target="_blank"
								rel="noreferrer"
							>
								{t("teamInstallDocker")}
							</a>
						</div>
					</div>
				)}
				{designer && included && (
					<section
						className={styles.memberCard}
						aria-labelledby={`preview-${index}`}
					>
						{editing || errors.some((e) => e.path.startsWith("/preview")) ? (
							<>
								<div className={pages.titled}>
									<h2 id={`preview-${index}`}>
										{t("previewSetupTitle", { designer: agent.displayName })}
									</h2>
									<InfoTip id={`preview-${index}-more-info`}>
										<Bullets text={t("previewMore")} />
									</InfoTip>
								</div>
								<Bullets
									text={t("previewSetupLead", { designer: agent.displayName })}
								/>
								<div id={`preview-${index}-fields`}>
									<PreviewFields
										id={`preview-${index}`}
										form={draft.preview}
										designer={agent.displayName}
										errors={errors}
										onChange={(preview) => {
											setErrors(
												errors.filter((e) => !e.path.startsWith("/preview")),
											);
											change({ ...draft, preview });
										}}
									/>
								</div>
							</>
						) : (
							<>
								<h2 id={`preview-${index}`}>{t("previewTitle")}</h2>
								<p>
									{t("previewFound", {
										designer: agent.displayName,
										start: draft.preview.start.trim(),
									})}
								</p>
								<Button
									kind="quiet"
									expanded={false}
									controls={`preview-${index}-fields`}
									onClick={() => {
										setEditing(true);
										requestAnimationFrame(() =>
											document
												.getElementById(`preview-${index}-start`)
												?.focus(),
										);
									}}
								>
									{t("previewChange")}
								</Button>
							</>
						)}
					</section>
				)}
			</li>
		);
	};

	return (
		<Wizard
			step={4}
			title={t("teamTitle")}
			lead={t(scratch ? "teamLeadScratch" : "teamLead")}
			carried={!!draft.from}
		>
			<Choice<Start>
				name="start"
				legend={t("startsLegend")}
				value={draft.start}
				onChange={(start) => begin(start)}
				options={[
					{
						value: "suggested",
						label: t("startSuggested"),
						description: t("startSuggestedNote"),
					},
					{
						value: "builtin",
						label: t("startBuiltin"),
						description: t("startBuiltinNote"),
						disabled: !builtin.length,
						after: draft.start === "builtin" && (
							<div className={styles.which}>
								<Choice<string>
									name="builtin"
									legend={t("startWhichBuiltin")}
									value={pick("builtin") ?? ""}
									onChange={(chosen) => begin("builtin", chosen)}
									options={builtin.map(({ slug, template }) => ({
										value: slug,
										label: template.name,
										description:
											slug in BUILTIN_LINES
												? t(BUILTIN_LINES[slug as keyof typeof BUILTIN_LINES])
												: builtinMeta(template),
										extra: <Faces agents={template.agents} />,
									}))}
								/>
							</div>
						),
					},
					{
						value: "saved",
						label: t("startSaved"),
						description: noFolder
							? t("templateNoFolder")
							: saved.length
								? t("startSavedNote")
								: t("startSavedNone"),
						disabled: !saved.length,
						after: draft.start === "saved" && (
							<div className={styles.which}>
								<Choice<string>
									name="saved"
									legend={t("startWhich")}
									value={pick("saved") ?? ""}
									onChange={(chosen) => begin("saved", chosen)}
									options={saved.map(({ slug, template }) => ({
										value: slug,
										label: template.name,
										description: templateMeta(template),
										extra: <Faces agents={template.agents} />,
									}))}
								/>
							</div>
						),
					},
					{
						value: "scratch",
						label: t("startScratch"),
						description: t("startScratchNote"),
					},
				]}
			/>
			{draft.from && <p>{t("teamLeadSaved", { name: draft.from })}</p>}
			<ul className={styles.members} aria-label={t("teamMembers")}>
				{draft.members.map(
					(member, index) => !member.more && row(member, index),
				)}
			</ul>
			<span>
				<Button kind="quiet" onClick={add}>
					{t("teamAdd")}
				</Button>
			</span>
			{draft.members.some((m) => m.more) && (
				<section className={styles.more} aria-labelledby="more-roles">
					<h2 id="more-roles">{t("setupMoreRoles")}</h2>
					<p>{t("setupMoreRolesNote")}</p>
					<ul className={styles.members} aria-labelledby="more-roles">
						{draft.members.map(
							(member, index) => member.more && row(member, index),
						)}
					</ul>
				</section>
			)}
			{draft.from && (
				<p className={styles.note}>
					{t("teamSavedAnswers", { name: draft.from })}
				</p>
			)}
			{loose.length > 0 && (
				<p role="alert" className={styles.alert}>
					{loose.map((e) => said(e.code)).join(" ")}
				</p>
			)}
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/scan")}>{t("back")}</Button>
				<span>
					{held && <span className={styles.note}>{t("teamNameAll")}</span>}
					<Button
						kind="primary"
						busy={busy}
						disabled={on.length === 0 || held}
						onClick={onward}
					>
						{t(
							on.length === 6 && draft.start === "suggested"
								? "teamContinueSix"
								: "teamContinue",
						)}
					</Button>
				</span>
			</div>
		</Wizard>
	);
}
