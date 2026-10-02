import { Button, Choice, Dialog, Switch, TextField } from "@farik/ui";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { type Refusal, said, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { ConnectorAdd, hostOf, labelsSaid } from "./ConnectorAdd.tsx";
import styles from "./pages.module.css";
import { useAdvanced } from "./Settings.tsx";
import {
	type Agent,
	type McpServer,
	roleName,
	someone,
	type Team as TeamFile,
} from "./setup/TeamSetup.tsx";
import {
	type ConnectorState,
	type Effective,
	type Model,
	type Tier,
	useStatus,
	useTeam,
} from "./Team.tsx";

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
 * custom connector added or removed and the agent's status, stays as the daemon last said (I3).
 */
function rebased(draft: Edited, saved: Edited): Edited {
	const builtins = (draft.mcpServers ?? []).filter(
		(c) => c.source !== "custom",
	);
	const kept = saved.mcpServers ?? [];
	const mcpServers = [
		...kept.flatMap((c) =>
			c.source === "custom" ? [c] : builtins.filter((b) => b.name === c.name),
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
	const { team, effective, models, connectors, sandboxed, again } = useTeam();
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
	sandboxed,
	again,
}: {
	team: TeamFile;
	saved: Edited;
	/** The daemon's answer for the saved agent: its model in words, and its tiers before its own changes. */
	known: Effective;
	models: Model[];
	connectors: ConnectorState[];
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
	const stateOf = (server: string) =>
		connectors.find((c) => c.agent === saved.id && c.server === server);
	const removeBody = {
		keychain: "connectorRemoveBodyKeychain",
		file: "connectorRemoveBodyFile",
		none: "connectorRemoveBody",
	} as const;
	/** What Remove says it deletes, and, for a sign-in, whether the service is asked to forget it. */
	const removeWords = (server: string) => {
		const kept = stateOf(server);
		if (kept?.auth !== "oauth")
			return t(removeBody[kept?.storedIn ?? "none"], { server, name });
		const host = hostOf(custom.find((c) => c.name === server)?.url);
		const forgets = kept.revokes !== false;
		const file = kept.storedIn === "file";
		return t(
			forgets
				? file
					? "connectorRemoveSignedFile"
					: "connectorRemoveSignedKeychain"
				: file
					? "connectorRemoveSignedFileStays"
					: "connectorRemoveSignedKeychainStays",
			{ host },
		);
	};
	const [adding, setAdding] = useState<{
		again?: McpServer;
		ended?: boolean;
	}>();
	const [removing, setRemoving] = useState<string>();
	const [removeRefused, setRemoveRefused] = useState<string>();
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
				{custom.length > 0 && (
					<>
						<h3 id="yours-heading" className={styles.subheading}>
							{t("connectorsYours")}{" "}
							<span className={styles.muted}>{t("connectorsYoursLead")}</span>
						</h3>
						<ul className={styles.ruled} aria-label={t("connectorsYours")}>
							{custom.map((c) => (
								<CustomRow
									key={c.name}
									server={c}
									state={stateOf(c.name)?.state}
									storedIn={stateOf(c.name)?.storedIn}
									auth={stateOf(c.name)?.auth}
									name={name}
									onAgain={() => setAdding({ again: c })}
									onSignInAgain={() => setAdding({ again: c, ended: true })}
									onRemove={() => {
										setRemoveRefused(undefined);
										setRemoving(c.name);
									}}
								/>
							))}
						</ul>
					</>
				)}
			</section>
			{removing && (
				<Dialog
					open
					title={t("connectorRemoveTitle", { server: removing, name })}
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
								{t("connectorRemoveLabel", { server: removing })}
							</Button>
						</>
					}
				>
					<p>{removeWords(removing)}</p>
					<p className={styles.muted}>
						{t(
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
			{adding && (
				<ConnectorAdd
					agent={saved.id}
					name={name}
					again={adding.again}
					ended={adding.ended}
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
	name,
	onAgain,
	onSignInAgain,
	onRemove,
}: {
	server: McpServer;
	state: ConnectorState["state"] | undefined;
	storedIn: ConnectorState["storedIn"];
	auth: ConnectorState["auth"];
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
				<p>{t("connectorSignedIn", { host })}</p>
			)}
			{signedIn && state === "sign_in_again" && (
				<p>
					<strong>{t("connectorSignInEnded", { host })}</strong>
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
