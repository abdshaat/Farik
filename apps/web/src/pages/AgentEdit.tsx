import { Button, Choice, Dialog, Switch, TextField } from "@farik/ui";
import { type ReactNode, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { type Refusal, said, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import {
	type AllowanceRow,
	type Allowances,
	countSaid,
	useAllowances,
} from "./allowances.tsx";
import { ConnectorAdd, hostOf, labelsSaid } from "./ConnectorAdd.tsx";
import { ConnectorAllowance } from "./dialogs/ConnectorAllowance.tsx";
import { type Editing, SkillEdit } from "./dialogs/SkillEdit.tsx";
import { SkillRead } from "./dialogs/SkillRead.tsx";
import { SkillReview } from "./dialogs/SkillReview.tsx";
import { visibly } from "./dialogs/ToolApproval.tsx";
import { KitConnect } from "./KitConnect.tsx";
import styles from "./pages.module.css";
import { useAdvanced } from "./Settings.tsx";
import { SitesSection } from "./Sites.tsx";
import {
	type Agent,
	type McpServer,
	roleName,
	someone,
	type Team as TeamFile,
} from "./setup/TeamSetup.tsx";
import {
	refusedWith,
	type SkillAt,
	type SkillFolder,
	type SkillShipped,
	sendSkill,
	skillParams,
} from "./skills.ts";
import {
	type ConnectorState,
	type Effective,
	type KitService,
	type Model,
	type RoleKit,
	type Tier,
	useStatus,
	useTeam,
} from "./Team.tsx";

/** One row of `skills.list`. */
type SkillRow = {
	level: "role" | "team" | "agent";
	name: string;
	description: string;
	state: "in_use" | "replaced" | "review" | "missing";
};

type Effort = "low" | "medium" | "high";
type Edited = Omit<Agent, "model"> & {
	model?: { id?: string; effort?: Effort } | undefined;
	grants?: Tier[] | undefined;
	revokes?: Tier[] | undefined;
};
type Checked = {
	errors: Refusal[];
	effects: string[];
};

/** Each tier, in the order the page lists them, with its words. */
/** The kit connector whose running ads Farik pauses before it is removed (step 08g). */
const GOOGLE_ADS = "google-ads";

const TIERS = [
	["read", "tierRead", "tierReadNote"],
	["write_workspace", "tierWrite", "tierWriteNote"],
	["execute", "tierExecute", "tierExecuteNote"],
	["git_local", "tierGitLocal", "tierGitLocalNote"],
	["network", "tierNetwork", "tierNetworkNote"],
	["git_remote", "tierGitRemote", "tierGitRemoteNote"],
	["external_effect", "tierExternal", "tierExternalNote"],
] as const;

/** The agent with `tier` on or off: a grant or revoke only where it differs from the base. */
function withTier(agent: Edited, base: Tier[], tier: Tier, on: boolean) {
	const grants = (agent.grants ?? []).filter((g) => g !== tier);
	const revokes = (agent.revokes ?? []).filter((r) => r !== tier);
	if (on && !base.includes(tier)) grants.push(tier);
	if (!on && base.includes(tier)) revokes.push(tier);
	return {
		...agent,
		grants: grants.length ? grants : undefined,
		revokes: revokes.length ? revokes : undefined,
	};
}

/** The built-in browser connector (spec 5.6), as `team.yaml` names it. */
const PLAYWRIGHT = { name: "playwright", source: "builtin" };

/** The agent with the Playwright connector on or off, its other connectors kept. */
function withPlaywright(agent: Edited, on: boolean): Edited {
	const others = (agent.mcpServers ?? []).filter(
		(c) => c.name !== PLAYWRIGHT.name,
	);
	const mcpServers = on ? [...others, PLAYWRIGHT] : others;
	const { mcpServers: _, ...rest } = agent;
	return mcpServers.length ? { ...rest, mcpServers } : rest;
}

/**
 * The draft's own edits laid over the agent as last read. What the page changes outside Save, a
 * custom or kit connector added or removed and the agent's status, stays as the daemon last said (I3).
 */
function rebased(draft: Edited, saved: Edited): Edited {
	const builtins = (draft.mcpServers ?? []).filter(
		(c) => c.source === "builtin",
	);
	const kept = saved.mcpServers ?? [];
	const mcpServers = [
		// A custom or a kit connector is as the daemon last said, whatever the draft holds.
		...kept.flatMap((c) =>
			c.source !== "builtin" ? [c] : builtins.filter((b) => b.name === c.name),
		),
		...builtins.filter((b) => !kept.some((c) => c.name === b.name)),
	];
	const { mcpServers: _, ...rest } = {
		...saved,
		displayName: draft.displayName,
		...(draft.persona !== undefined && { persona: draft.persona }),
		model: draft.model,
		grants: draft.grants,
		revokes: draft.revokes,
	};
	return mcpServers.length ? { ...rest, mcpServers } : rest;
}

/** One agent's page, once the team has loaded. */
export function AgentEdit() {
	const { id } = useParams();
	const { team, effective, models, connectors, kits, sandboxed, again } =
		useTeam();
	if (!team) return null;
	const saved = team.agents.find((a) => a.id === id);
	const known = effective.find((e) => e.id === id);
	if (!saved || !known)
		return (
			<div className={styles.page}>
				<p>{t("agentMissing").replace("{id}", id ?? "")}</p>
				<Link to="/team">{t("agentBack")}</Link>
			</div>
		);
	return (
		<Editor
			key={saved.id}
			team={team}
			saved={saved as Edited}
			known={known}
			models={models}
			connectors={connectors}
			kits={kits}
			sandboxed={sandboxed}
			again={again}
		/>
	);
}

/** Name, voice, effort, model and tiers, each change described before Save (spec 10). */
function Editor({
	team,
	saved,
	known,
	models,
	connectors,
	kits,
	sandboxed,
	again,
}: {
	team: TeamFile;
	saved: Edited;
	/** The daemon's answer for the saved agent: its model in words, and its tiers before its own changes. */
	known: Effective;
	models: Model[];
	connectors: ConnectorState[];
	kits: RoleKit[];
	/** Whether sessions run in Docker's sandbox (team.get). */
	sandboxed: boolean;
	/** Reads the team again, after a connector is added or removed outside Save. */
	again: () => void;
}) {
	const { client } = useConnection();
	const navigate = useNavigate();
	const status = useStatus();
	const [advanced, setAdvanced] = useAdvanced();
	const [draft, setDraft] = useState<Edited>();
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const agent = draft ? rebased(draft, saved) : saved;
	const next: TeamFile = {
		...team,
		agents: team.agents.map((a) => (a.id === saved.id ? agent : a)),
	};
	const { data: checked } = useQuery<Checked>("team.validate", { team: next });

	const name = saved.displayName;
	const say = (key: Parameters<typeof t>[0]) =>
		t(key).replaceAll("{name}", name).replace("{role}", roleName(agent.role));
	const changed = JSON.stringify(agent) !== JSON.stringify(saved);
	const errors = changed ? (checked?.errors ?? []) : [];
	const effects = changed ? (checked?.effects ?? []) : [];
	const base = known.baseTiers;
	const tiers = new Set<Tier>([...base, ...(agent.grants ?? [])]);
	for (const r of agent.revokes ?? []) tiers.delete(r);
	const effort = agent.model?.effort ?? known.model.effort;
	const modelId = agent.model?.id ?? "";
	// The agent's own model in words when the list has no newer label for it, never its id.
	const options =
		models.some((m) => m.id === modelId) || !modelId
			? models
			: [
					{
						id: modelId,
						label: modelId === known.model.id ? known.model.label : modelId,
					},
					...models,
				];

	const save = async (team: TeamFile) => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("team.save", { team });
			navigate("/team");
		} catch (e) {
			setRefused(saidAll(e));
			setBusy(false);
		}
	};
	const replace = async () => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			// One call: the team is never without the role, even for a moment.
			await client.call("agent.replace", {
				agentId: saved.id,
				newcomer: { ...someone(team.agents, saved), status: "active" },
			});
			navigate("/team");
		} catch (e) {
			setRefused(saidAll(e));
			setBusy(false);
		}
	};
	const retire = async () => {
		if (await status.set(saved, "retired")) navigate("/team");
	};
	const paused = saved.status === "paused";
	const custom = (saved.mcpServers ?? []).filter((c) => c.source === "custom");
	// What Farik offers this role, and the kit services this agent holds that it no longer offers.
	const offered: KitService[] =
		kits.find((kit) => kit.role === saved.role)?.connectors ?? [];
	const heldFromKit = (saved.mcpServers ?? []).filter(
		(c) => c.source === "kit",
	);
	const gone = heldFromKit.filter(
		(c) => !offered.some((service) => service.name === c.name),
	);
	const stateOf = (server: string) =>
		connectors.find((c) => c.agent === saved.id && c.server === server);
	/** What Remove calls a connector: Google Ads by the kit's title, as its board draws it. */
	const removalName = (server: string) =>
		server === GOOGLE_ADS
			? (offered.find((one) => one.name === server)?.title ?? server)
			: server;
	const removeBody = {
		keychain: "connectorRemoveBodyKeychain",
		file: "connectorRemoveBodyFile",
		none: "connectorRemoveBody",
	} as const;
	/** What Remove says it deletes, and, for a sign-in, whether the service is asked to forget it. */
	const removeWords = (server: string): ReactNode => {
		const kept = stateOf(server);
		if (kept?.auth !== "oauth")
			return t(removeBody[kept?.storedIn ?? "none"], { server, name });
		// Who signed the agent in: Farik's own app by name, else the address's host, else (a
		// connector with no web address) the connector's own name.
		const url = (saved.mcpServers ?? []).find((c) => c.name === server)?.url;
		const host = kept.provider ?? (hostOf(url) || server);
		const forgets = kept.revokes !== false;
		const file = kept.storedIn === "file";
		if (forgets)
			return t(
				file ? "connectorRemoveSignedFile" : "connectorRemoveSignedKeychain",
				{ host },
			);
		// Where Farik is removed at the service, as a link when the app knows the page.
		const settings = t("connectorSettings", { host });
		const [before = "", after = ""] = t(
			file
				? "connectorRemoveSignedFileStays"
				: "connectorRemoveSignedKeychainStays",
		).split("{settings}");
		return (
			<>
				{before}
				{kept.settingsUrl?.startsWith("https://") ? (
					<a href={kept.settingsUrl} target="_blank" rel="noopener noreferrer">
						{settings}
					</a>
				) : (
					settings
				)}
				{after}
			</>
		);
	};
	const [adding, setAdding] = useState<{
		again?: McpServer;
		ended?: boolean;
		/** Who ended the sign-in, when Farik's own app signed it in: the provider, not the address. */
		endedBy?: string;
		kit?: KitService;
	}>();
	const [removing, setRemoving] = useState<string>();
	const [removeRefused, setRemoveRefused] = useState<string>();
	// "Change how many" opens on a service; the approval dialog's link opens it from outside.
	const allowances = useAllowances();
	const [search] = useSearchParams();
	const [changing, setChanging] = useState<string | undefined>(
		search.get("allowances") ?? undefined,
	);
	const allowanceRows = (server: string): AllowanceRow[] =>
		(allowances?.rows ?? []).filter(
			(row) => row.agent === saved.id && row.server === server,
		);
	const remove = async (server: string) => {
		if (!client) return;
		setBusy(true);
		setRemoveRefused(undefined);
		try {
			await client.call("connector.disconnect", { agent: saved.id, server });
			setRemoving(undefined);
			again();
		} catch (e) {
			setRemoveRefused(saidAll(e));
		}
		setBusy(false);
	};
	const browsing = (agent.mcpServers ?? []).some(
		(c) => c.name === PLAYWRIGHT.name,
	);

	return (
		<div className={styles.page}>
			<Link to="/team">{t("agentBack")}</Link>
			<h1 className={styles.title}>{say("agentTitle")}</h1>
			<TextField
				id="agent-name"
				label={t("agentName")}
				value={agent.displayName}
				onChange={(displayName) => setDraft({ ...agent, displayName })}
			/>
			<TextField
				id="agent-persona"
				label={say("agentTalks")}
				hint={say("agentTalksHint")}
				value={agent.persona ?? ""}
				onChange={(persona) => setDraft({ ...agent, persona })}
			/>
			<Choice<Effort>
				name="effort"
				legend={say("agentEffort")}
				value={effort}
				onChange={(e) =>
					// How carefully an agent works never changes what it runs on.
					setDraft({
						...agent,
						model: { ...(modelId && { id: modelId }), effort: e },
					})
				}
				options={[
					{
						value: "low",
						label: t("effortLow"),
						description: t("effortLowNote"),
					},
					{
						value: "medium",
						label: t("effortMedium"),
						description: t("effortMediumNote"),
					},
					{
						value: "high",
						label: t("effortHigh"),
						description: t("effortHighNote"),
					},
				]}
			/>
			<div className={styles.field}>
				<label htmlFor="agent-model">{t("agentModel")}</label>
				<select
					id="agent-model"
					className={styles.select}
					value={modelId}
					onChange={(e) =>
						setDraft({
							...agent,
							model: {
								...(e.target.value && { id: e.target.value }),
								...(agent.model?.effort && { effort: agent.model.effort }),
							},
						})
					}
				>
					{!modelId && (
						<option value="">{`${t("agentModelRole")}: ${known.model.label}`}</option>
					)}
					{options.map((m) => (
						<option key={m.id} value={m.id}>
							{advanced ? `${m.label} (${m.id})` : m.label}
						</option>
					))}
				</select>
			</div>
			{/* The Procurement Specialist reads only the sites it is given: they come before what it may do. */}
			{saved.role === "procurement_specialist" && <SitesSection name={name} />}
			<section className={styles.section} aria-labelledby="may-heading">
				<h2 id="may-heading">{say("agentMay")}</h2>
				<Switch
					id="agent-advanced"
					label={t("advancedSwitch")}
					checked={advanced}
					onChange={setAdvanced}
				/>
				{advanced ? (
					<ul className={styles.tiers} aria-label={say("agentMay")}>
						{TIERS.map(([tier, label, note]) => (
							<li key={tier}>
								<Switch
									id={`tier-${tier}`}
									label={t(label)}
									description={t(note)}
									checked={tiers.has(tier)}
									onChange={(on) => setDraft(withTier(agent, base, tier, on))}
								/>
							</li>
						))}
					</ul>
				) : (
					<p className={styles.muted}>{t("agentMayAdvanced")}</p>
				)}
				{advanced && (
					<>
						<h3 className={styles.subheading}>{t("connectorCustom")}</h3>
						<p>{t("connectorCustomNote")}</p>
						<span>
							<Button onClick={() => setAdding({})}>
								{t("connectorCustomAdd")}
							</Button>
						</span>
						<p className={styles.muted}>{t("connectorCustomKeychain")}</p>
					</>
				)}
				<span>
					<Button
						kind="quiet"
						onClick={() =>
							setDraft({
								...agent,
								model: undefined,
								grants: undefined,
								revokes: undefined,
							})
						}
					>
						{t("agentDefaults")}
					</Button>
				</span>
			</section>
			<section className={styles.section} aria-labelledby="connectors-heading">
				<h2 id="connectors-heading">
					{t("connectors")}{" "}
					<span className={styles.muted}>{t("connectorsLead", { name })}</span>
				</h2>
				<Switch
					id="connector-playwright"
					label={t("connectorPlaywright")}
					description={t("connectorPlaywrightNote", { name })}
					checked={browsing}
					onChange={(on) => setDraft(withPlaywright(agent, on))}
				/>
				{!browsing && agent.role === "ui_ux_designer" && (
					<p>
						<strong>{t("connectorOff", { name })}</strong>
					</p>
				)}
				<p className={styles.muted}>{t("connectorsNote")}</p>
				{(offered.length > 0 || gone.length > 0) && (
					<>
						<h3 id="kit-heading" className={styles.subheading}>
							{t("kitHeading", { role: roleName(agent.role) })}{" "}
							<span className={styles.muted}>{t("kitLead")}</span>
						</h3>
						<ul
							className={styles.ruled}
							aria-label={t("kitHeading", { role: roleName(agent.role) })}
						>
							{offered.map((service) => {
								const held = heldFromKit.find((c) => c.name === service.name);
								return (
									<KitRow
										key={service.name}
										service={service}
										held={held}
										state={stateOf(service.name)?.state}
										storedIn={stateOf(service.name)?.storedIn}
										provider={stateOf(service.name)?.provider}
										name={name}
										made={allowanceRows(service.name)}
										period={allowances?.period}
										onChange={() => setChanging(service.name)}
										onConnect={() =>
											setAdding({ ...(held && { again: held }), kit: service })
										}
										onRemove={() => {
											setRemoveRefused(undefined);
											setRemoving(service.name);
										}}
									/>
								);
							})}
							{gone.map((c) => (
								<li key={c.name} className={styles.kitRow}>
									<strong>{c.name}</strong>
									<p>
										<strong className={styles.warn}>{t("kitGone")}</strong>
									</p>
									<p className={styles.muted}>{t("kitGoneNote", { name })}</p>
									<span className={styles.actions}>
										<Button
											kind="quiet"
											onClick={() => {
												setRemoveRefused(undefined);
												setRemoving(c.name);
											}}
										>
											{t("connectorRemove")}{" "}
											<span className={styles.hidden}>{c.name}</span>
										</Button>
									</span>
								</li>
							))}
						</ul>
					</>
				)}
				{custom.length > 0 && (
					<>
						<h3 id="yours-heading" className={styles.subheading}>
							{t("connectorsYours")}{" "}
							<span className={styles.muted}>{t("connectorsYoursLead")}</span>
						</h3>
						<ul className={styles.ruled} aria-label={t("connectorsYours")}>
							{custom.map((c) => {
								const provider = stateOf(c.name)?.provider;
								return (
									<CustomRow
										key={c.name}
										server={c}
										state={stateOf(c.name)?.state}
										storedIn={stateOf(c.name)?.storedIn}
										auth={stateOf(c.name)?.auth}
										provider={provider}
										name={name}
										onAgain={() => setAdding({ again: c })}
										onSignInAgain={() =>
											setAdding({
												again: c,
												ended: true,
												...(provider && { endedBy: provider }),
											})
										}
										onRemove={() => {
											setRemoveRefused(undefined);
											setRemoving(c.name);
										}}
									/>
								);
							})}
						</ul>
					</>
				)}
			</section>
			<SkillsSection
				agent={saved.id}
				name={name}
				role={roleName(agent.role)}
				roleId={agent.role}
				onChanged={again}
			/>
			{removing && (
				<Dialog
					open
					title={t("connectorRemoveTitle", {
						server: removalName(removing),
						name,
					})}
					onClose={() => setRemoving(undefined)}
					actions={
						<>
							<Button onClick={() => setRemoving(undefined)}>
								{t("connectorKeep")}
							</Button>
							<Button
								kind="primary"
								busy={busy}
								onClick={() => remove(removing)}
							>
								{t("connectorRemoveLabel", { server: removalName(removing) })}
							</Button>
						</>
					}
				>
					{/* Google Ads holds ads that run at Google: Farik pauses them before it lets go. */}
					{removing === GOOGLE_ADS && <p>{t("connectorRemoveGoogleAds")}</p>}
					<p>{removeWords(removing)}</p>
					<p className={styles.muted}>
						{removing === GOOGLE_ADS
							? t("connectorRemoveAgainSigned")
							: t(
									stateOf(removing)?.auth === "oauth"
										? "connectorRemoveOthersSigned"
										: "connectorRemoveOthers",
								)}
					</p>
					{removeRefused && (
						<p role="alert" className={styles.alert}>
							{removeRefused}
						</p>
					)}
				</Dialog>
			)}
			{changing &&
				(() => {
					const service = offered.find((one) => one.name === changing);
					const rows = allowanceRows(changing);
					return service && rows.length > 0 ? (
						<ConnectorAllowance
							agent={saved.id}
							name={name}
							service={service}
							rows={rows}
							onClose={(changed) => {
								setChanging(undefined);
								if (changed) again();
							}}
							onAgain={() => {
								setChanging(undefined);
								const held = heldFromKit.find((c) => c.name === service.name);
								setAdding({ ...(held && { again: held }), kit: service });
							}}
						/>
					) : null;
				})()}
			{adding?.kit && (
				<KitConnect
					agent={saved.id}
					name={name}
					service={adding.kit}
					sandboxed={sandboxed}
					onClose={(changed) => {
						setAdding(undefined);
						if (changed) again();
					}}
				/>
			)}
			{adding && !adding.kit && (
				<ConnectorAdd
					agent={saved.id}
					name={name}
					again={adding.again}
					ended={adding.ended}
					endedBy={adding.endedBy}
					sandboxed={sandboxed}
					onClose={(changed) => {
						setAdding(undefined);
						if (changed) again();
					}}
				/>
			)}
			{effects.length > 0 && (
				<section
					className={styles.section}
					aria-labelledby="effects-heading"
					aria-live="polite"
				>
					<h2 id="effects-heading">{t("agentEffects")}</h2>
					<ul>
						{effects.map((e) => (
							<li key={e}>{e}</li>
						))}
					</ul>
				</section>
			)}
			{(errors.length > 0 || refused || status.refusal) && (
				<p role="alert" className={styles.alert}>
					{[...errors.map((e) => said(e.code)), refused, status.refusal]
						.filter(Boolean)
						.join(" ")}
				</p>
			)}
			<div className={styles.actions}>
				<Button
					kind="primary"
					busy={busy}
					disabled={!changed || errors.length > 0}
					onClick={() => save(next)}
				>
					{t("agentSave")}
				</Button>
				<Button onClick={() => setDraft(undefined)} disabled={!changed}>
					{t("agentCancel")}
				</Button>
			</div>
			<p className={styles.muted}>{say("agentNextWork")}</p>
			<section className={styles.section} aria-labelledby="place-heading">
				<h2 id="place-heading">{say("agentPlace")}</h2>
				<div className={styles.place}>
					<Button
						onClick={() => status.set(saved, paused ? "active" : "paused")}
					>
						{say(paused ? "agentResume" : "agentPause")}
					</Button>
					<p>{say("agentPauseNote")}</p>
					<Button onClick={replace}>{say("agentReplace")}</Button>
					<p>{say("agentReplaceNote")}</p>
					<Button onClick={retire}>{say("agentRetire")}</Button>
					<p>{say("agentRetireNote")}</p>
				</div>
			</section>
		</div>
	);
}

/** One of "Added by you": how it starts, its tools by label, and whether it runs here. */
function CustomRow({
	server,
	state,
	storedIn,
	auth,
	provider,
	name,
	onAgain,
	onSignInAgain,
	onRemove,
}: {
	server: McpServer;
	state: ConnectorState["state"] | undefined;
	storedIn: ConnectorState["storedIn"];
	auth: ConnectorState["auth"];
	/** Farik's own app that signed the agent in, when one did. */
	provider: ConnectorState["provider"];
	name: string;
	onAgain: () => void;
	onSignInAgain: () => void;
	onRemove: () => void;
}) {
	const signedIn = auth === "oauth" || server.oauth !== undefined;
	const host = hostOf(server.url);
	const tags = Object.values(server.tools ?? {});
	const labels = labelsSaid(tags);
	return (
		<li>
			<div className={styles.rowHead}>
				<span>
					<strong>{server.name}</strong>{" "}
					<span className={styles.muted}>
						{t(
							server.transport === "http" ? "connectorHttp" : "connectorStdio",
						)}
					</span>
				</span>
				<span className={styles.actions}>
					{state === "connect_again" && (
						<Button onClick={onAgain}>{t("connectorAgainButton")}</Button>
					)}
					{state === "sign_in_again" && (
						<Button onClick={onSignInAgain}>{t("connectorSignInAgain")}</Button>
					)}
					<Button kind="quiet" onClick={onRemove}>
						{t("connectorRemove")}{" "}
						<span className={styles.hidden}>{server.name}</span>
					</Button>
				</span>
			</div>
			{signedIn && state === "connected" && (
				<p>{t("connectorSignedIn", { host: provider ?? host })}</p>
			)}
			{signedIn && state === "sign_in_again" && (
				<p>
					<strong>
						{t("connectorSignInEnded", { host: provider ?? host })}
					</strong>
				</p>
			)}
			{!signedIn && storedIn && (
				<p>
					{t(storedIn === "file" ? "connectorFile" : "connectorKeychain", {
						name,
					})}
				</p>
			)}
			<p className={styles.muted}>
				{tags.length === 1
					? t("connectorOneTool", { labels })
					: t("connectorTools", { count: tags.length, labels })}
			</p>
			{state === "connect_again" && (
				<>
					<p>
						<strong>{t("connectorAgain")}</strong>
					</p>
					<p className={styles.muted}>{t("connectorAgainNote", { name })}</p>
				</>
			)}
			{state === "store_unavailable" && (
				<>
					<p>
						<strong>
							{t("connectorUnreadable", { name, server: server.name })}
						</strong>
					</p>
					<p className={styles.muted}>
						{t("connectorUnreadableNote", { name, server: server.name })}
					</p>
				</>
			)}
		</li>
	);
}

/** One service of the role's kit: why the role wants it, how it stands, and what can be done (ADR 0036). */
function KitRow({
	service,
	held,
	state,
	storedIn,
	provider,
	name,
	made,
	period,
	onChange,
	onConnect,
	onRemove,
}: {
	service: KitService;
	held: McpServer | undefined;
	state: ConnectorState["state"] | undefined;
	storedIn: ConnectorState["storedIn"];
	/** Farik's own app that signed the agent in, when one did: it, not the service's title, signed it in. */
	provider: ConnectorState["provider"];
	name: string;
	/** What the agent has made of this service's spending tools this period. */
	made: AllowanceRow[];
	period: Allowances["period"] | undefined;
	onChange: () => void;
	onConnect: () => void;
	onRemove: () => void;
}) {
	const connected = held !== undefined && state === "connected";
	return (
		<li className={styles.kitRow}>
			<strong>{service.title}</strong>
			<p className={styles.muted}>{service.why}</p>
			{service.atLaunch && !held && (
				<>
					<p className={styles.muted}>{service.about}</p>
					<p>
						<strong>{t("kitAtLaunch", { service: service.title })}</strong>
					</p>
				</>
			)}
			{connected && (
				<>
					<p>
						<strong>{t("kitConnected")}</strong>
					</p>
					<p className={styles.muted}>
						{service.auth === "oauth"
							? t("kitSignedIn", { service: provider ?? service.title })
							: t(storedIn === "file" ? "connectorFile" : "connectorKeychain", {
									name,
								})}
					</p>
				</>
			)}
			{connected && made.length > 0 && (
				<p>
					{t(period?.kind === "day" ? "allowRowDay" : "allowRowSprint", {
						list: made.map(countSaid).join(", "),
					})}
				</p>
			)}
			{held && state === "connect_again" && (
				<p>
					<strong className={styles.warn}>{t("kitAgain")}</strong>
				</p>
			)}
			{held && state === "sign_in_again" && (
				<p>
					<strong>
						{t("connectorSignInEnded", {
							host: provider ?? service.title,
						})}
					</strong>
				</p>
			)}
			{held && state === "store_unavailable" && (
				<p>
					<strong>
						{t("connectorUnreadable", { name, server: held.name })}
					</strong>
				</p>
			)}
			<span className={styles.actions}>
				{!held && !service.atLaunch && (
					<Button onClick={onConnect}>
						{t("kitConnect")}{" "}
						<span className={styles.hidden}>{service.title}</span>
					</Button>
				)}
				{connected && made.length > 0 && (
					<Button kind="quiet" onClick={onChange}>
						{t("allowChange")}{" "}
						<span className={styles.hidden}>
							{t("allowChangeHidden", { name, service: service.title })}
						</span>
					</Button>
				)}
				{held && state === "connect_again" && (
					<Button onClick={onConnect}>
						{t("connectorAgainButton")}{" "}
						<span className={styles.hidden}>{service.title}</span>
					</Button>
				)}
				{held && state === "sign_in_again" && (
					<Button onClick={onConnect}>
						{t("connectorSignInAgain")}{" "}
						<span className={styles.hidden}>{service.title}</span>
					</Button>
				)}
				{held && (
					<Button kind="quiet" onClick={onRemove}>
						{t("connectorRemove")}{" "}
						<span className={styles.hidden}>{held.name}</span>
					</Button>
				)}
			</span>
		</li>
	);
}

/** "Skills": what comes with the role, what the team has and what the agent has of its own. */
function SkillsSection({
	agent,
	name,
	role,
	roleId,
	onChanged,
}: {
	agent: string;
	name: string;
	role: string;
	/** The role's id, which `skill.get` asks a shipped skill by. */
	roleId: string;
	/** Reads the team again, which holds each skill's pin. */
	onChanged: () => void;
}) {
	const { client } = useConnection();
	const { data, again } = useQuery<{ skills: SkillRow[] }>("skills.list", {
		agent,
	});
	const [dialog, setDialog] = useState<
		| { kind: "edit"; editing?: Editing }
		| { kind: "review"; skill: SkillAt }
		| { kind: "remove"; skill: SkillAt }
		| { kind: "read"; skill: SkillShipped }
	>();
	const [refused, setRefused] = useState<string>();
	const [busy, setBusy] = useState(false);
	const rows = data?.skills ?? [];
	const shipped = rows.filter((r) => r.level === "role").map((r) => r.name);
	const done = (changed: boolean) => {
		setDialog(undefined);
		if (changed) {
			again();
			onChanged();
		}
	};
	const edit = async (skill: SkillAt) => {
		if (!client) return;
		setRefused(undefined);
		try {
			const folder = (await client.query(
				"skill.get",
				skillParams(agent, skill),
			)) as SkillFolder;
			setDialog({ kind: "edit", editing: { ...skill, folder } });
		} catch (e) {
			setRefused(
				said(refusedWith(e), { skill: skill.name }, "skillCannotOpen"),
			);
		}
	};
	const remove = async (skill: SkillAt) => {
		if (!client) return;
		setBusy(true);
		const code = await sendSkill(client, {
			command: "skill_remove",
			body: {
				level: skill.level,
				...(skill.level === "agent" && { agent }),
				name: skill.name,
			},
		});
		setBusy(false);
		if (code === undefined) done(true);
		else setRefused(said(code, {}, "skillOtherRefusal"));
	};
	const groups = [
		["role", t("skillsRole", { role })],
		["team", t("skillsTeam")],
		["agent", t("skillsOwn", { name })],
	] as const;
	return (
		<section className={styles.section} aria-labelledby="skills-heading">
			<h2 id="skills-heading">
				{t("skills")}{" "}
				<span className={styles.muted}>{t("skillsLead", { name })}</span>
			</h2>
			{groups.map(([level, heading]) => {
				const here = rows.filter((r) => r.level === level);
				if (!here.length) return null;
				return (
					<div key={level}>
						<h3 className={styles.subheading}>{heading}</h3>
						<ul className={styles.ruled} aria-label={heading}>
							{here.map((r) => {
								const skill: SkillAt | undefined =
									r.level === "role"
										? undefined
										: { level: r.level, name: r.name };
								return (
									<li key={r.name}>
										<div className={styles.rowHead}>
											<span>
												<strong>{r.name}</strong>
											</span>
											{r.level === "role" && (
												<span className={styles.actions}>
													<Button
														kind="quiet"
														onClick={() =>
															setDialog({
																kind: "read",
																skill: {
																	level: "role",
																	name: r.name,
																	role: roleId,
																},
															})
														}
													>
														{t("skillReadButton")}{" "}
														<span className={styles.hidden}>{r.name}</span>
													</Button>
												</span>
											)}
											{skill && (
												<span className={styles.actions}>
													{r.state === "review" && (
														<Button
															onClick={() =>
																setDialog({ kind: "review", skill })
															}
														>
															{t("skillReviewButton")}{" "}
															<span className={styles.hidden}>{r.name}</span>
														</Button>
													)}
													{(r.state === "in_use" || r.state === "replaced") && (
														<Button kind="quiet" onClick={() => edit(skill)}>
															{t("skillEditButton")}{" "}
															<span className={styles.hidden}>{r.name}</span>
														</Button>
													)}
													{r.state !== "review" && (
														<Button
															kind="quiet"
															onClick={() =>
																setDialog({ kind: "remove", skill })
															}
														>
															{t("skillRemoveButton")}{" "}
															<span className={styles.hidden}>{r.name}</span>
														</Button>
													)}
												</span>
											)}
										</div>
										{r.state === "missing" ? (
											<p>
												<strong>
													<code>{r.name}</code> {t("skillMissing")}
												</strong>
											</p>
										) : (
											<p>{visibly(r.description)}</p>
										)}
										{r.state === "review" && (
											<p>
												<strong>{t("skillToReview", { name })}</strong>
											</p>
										)}
										{r.state === "replaced" && (
											<p className={styles.muted}>
												{t(
													r.level !== "role"
														? "skillReplacedByOwn"
														: rows.some(
																	(o) =>
																		o.level === "agent" &&
																		o.name === r.name &&
																		o.state === "in_use",
																)
															? "skillReplacedByOwnRole"
															: "skillReplacedByTeam",
													{ name, skill: r.name },
												)}
											</p>
										)}
									</li>
								);
							})}
						</ul>
					</div>
				);
			})}
			{refused && (
				<p role="alert" className={styles.alert}>
					{refused}
				</p>
			)}
			<span>
				<Button onClick={() => setDialog({ kind: "edit" })}>
					{t("skillsAdd")}
				</Button>
			</span>
			{dialog?.kind === "edit" && (
				<SkillEdit
					agent={agent}
					name={name}
					role={role}
					shipped={shipped}
					editing={dialog.editing}
					onClose={done}
				/>
			)}
			{dialog?.kind === "review" && (
				<SkillReview
					agent={agent}
					name={name}
					role={role}
					skill={dialog.skill}
					shipped={shipped}
					onClose={done}
				/>
			)}
			{dialog?.kind === "read" && (
				<SkillRead
					agent={agent}
					skill={dialog.skill}
					onClose={() => setDialog(undefined)}
				/>
			)}
			{dialog?.kind === "remove" && (
				<Dialog
					open
					title={t("skillRemoveTitle", { skill: dialog.skill.name })}
					onClose={() => setDialog(undefined)}
					actions={
						<>
							<Button onClick={() => setDialog(undefined)}>
								{t("connectorKeep")}
							</Button>
							<Button
								kind="primary"
								busy={busy}
								onClick={() => remove(dialog.skill)}
							>
								{t("skillRemoveYes", { skill: dialog.skill.name })}
							</Button>
						</>
					}
				>
					<p>
						{dialog.skill.level === "agent"
							? t("skillRemoveOwn", { name })
							: t("skillRemoveTeam")}
					</p>
				</Dialog>
			)}
		</section>
	);
}
