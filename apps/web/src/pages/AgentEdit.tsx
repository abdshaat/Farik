import { Button, Choice, Switch, TextField } from "@farik/ui";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { type Refusal, said, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import { useAdvanced } from "./Settings.tsx";
import {
	type Agent,
	roleName,
	someone,
	type Team as TeamFile,
} from "./setup/TeamSetup.tsx";
import { type Model, useStatus, useTeam } from "./Team.tsx";

type Effort = "low" | "medium" | "high";
type Tier =
	| "read"
	| "write_workspace"
	| "execute"
	| "git_local"
	| "network"
	| "git_remote"
	| "external_effect";
type Edited = Omit<Agent, "model"> & {
	model?: { id: string; effort?: Effort } | undefined;
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

// ponytail: mirrors farik_core::governor::permissions::default_tiers; a tiers query replaces it.
const ROLE_TIERS: Record<Agent["role"], Tier[]> = {
	software_developer: ["read", "write_workspace", "execute", "git_local"],
	architect: ["read", "write_workspace", "execute", "network", "git_local"],
	product_manager: ["read", "network"],
	marketing_specialist: ["read", "network", "write_workspace", "git_local"],
	scrum_master: ["read"],
};

/** The tiers the role and the team's two permission answers give, before the agent's own changes. */
function baseTiers(agent: Edited, team: TeamFile): Tier[] {
	const { runCommands = true, push = false } = team.policy.permissions ?? {};
	const coder = agent.role === "software_developer";
	const tiers = ROLE_TIERS[agent.role].filter(
		(tier) =>
			runCommands ||
			tier !== "execute" ||
			!(coder || agent.role === "architect"),
	);
	return push && coder ? [...tiers, "git_remote"] : tiers;
}

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

/** One agent's page, once the team has loaded. */
export function AgentEdit() {
	const { id } = useParams();
	const { team, models } = useTeam();
	if (!team) return null;
	const saved = team.agents.find((a) => a.id === id);
	if (!saved)
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
			models={models}
		/>
	);
}

/** Name, voice, effort, model and tiers, each change described before Save (spec 10). */
function Editor({
	team,
	saved,
	models,
}: {
	team: TeamFile;
	saved: Edited;
	models: Model[];
}) {
	const { client } = useConnection();
	const navigate = useNavigate();
	const status = useStatus();
	const [advanced, setAdvanced] = useAdvanced();
	const [draft, setDraft] = useState<Edited>();
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const agent = draft ?? saved;
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
	const base = baseTiers(agent, team);
	const tiers = new Set<Tier>([...base, ...(agent.grants ?? [])]);
	for (const r of agent.revokes ?? []) tiers.delete(r);
	const effort = agent.model?.effort ?? "medium";
	const modelId = agent.model?.id ?? "";
	const options =
		models.some((m) => m.id === modelId) || !modelId
			? models
			: [{ id: modelId, label: modelId }, ...models];

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
		if (!(await status.set(saved, "retired"))) return;
		const newcomer = someone(team.agents, saved);
		await save({
			...team,
			agents: [
				...team.agents.map((a) =>
					a.id === saved.id ? { ...a, status: "retired" } : a,
				),
				{ ...newcomer, status: "active" },
			],
		});
	};
	const retire = async () => {
		if (await status.set(saved, "retired")) navigate("/team");
	};
	const paused = saved.status === "paused";

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
					setDraft({
						...agent,
						model: { id: modelId || (models[0]?.id ?? ""), effort: e },
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
						setDraft({ ...agent, model: { id: e.target.value, effort } })
					}
				>
					{!modelId && <option value="">{t("agentModelRole")}</option>}
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
