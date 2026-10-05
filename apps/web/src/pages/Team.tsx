import { AVATAR_URLS, type AvatarKey, Button, RoleTag } from "@farik/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { commandSaid, saidAll } from "../app/refusals.ts";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import { SaveTemplate } from "./dialogs/SaveTemplate.tsx";
import { UseTemplate } from "./dialogs/UseTemplate.tsx";
import styles from "./pages.module.css";
import {
	type Agent,
	ringOf,
	roleName,
	someone,
	type Team as TeamFile,
} from "./setup/TeamSetup.tsx";

/** The roles "Add someone" offers, in the order the team builder suggests them. */
const ROLES: Agent["role"][] = [
	"software_developer",
	"product_manager",
	"scrum_master",
	"architect",
	"ui_ux_designer",
	"marketing_specialist",
	"finance_specialist",
];

export type Model = { id: string; label: string };
export type Tier =
	| "read"
	| "write_workspace"
	| "execute"
	| "git_local"
	| "network"
	| "git_remote"
	| "external_effect";
/** What the daemon works out for an agent: its model as its sessions run it, in words, and its tiers. */
export type Effective = {
	id: string;
	model: Model & { effort: "low" | "medium" | "high" };
	tiers: Tier[];
	baseTiers: Tier[];
};
/** A service a role's kit offers, with the copy the page shows while connecting it (ADR 0036). */
export type KitService = {
	name: string;
	title: string;
	about: string;
	why: string;
	setup: string;
	/** Where the key is made; present when the service takes keys. */
	keyPage?: string;
	/** A tool to what it does in the user's words. */
	labels: Record<string, string>;
	auth: "keys" | "oauth";
	credentialKeys: string[];
	/** What the kit lets the user pre-approve: each spending tool with its default calls each sprint (ADR 0037). */
	allowances?: { tool: string; calls: number; what: string }[];
};
/** The services Farik offers one role. */
export type RoleKit = { role: Agent["role"]; connectors: KitService[] };
/** A custom connector's state on this computer, as `team.get` answers it. */
export type ConnectorState = {
	agent: string;
	server: string;
	state:
		| "connected"
		| "connect_again"
		| "sign_in_again"
		| "store_unavailable"
		| "not_in_kit";
	/** Whether the user added it (`custom`) or it is a service of the agent's role's kit (ADR 0036). */
	source?: "custom" | "kit";
	/** Whether the user gave keys or signed in to the service (ADR 0033). */
	auth?: "keys" | "oauth";
	/** For a sign-in: whether the service can be asked to forget it when it is removed. */
	revokes?: boolean;
	/** Where its keys or sign-in are kept, whenever some are. */
	storedIn?: "keychain" | "file";
};
type Holder = { agentId: string; displayName: string; role: Agent["role"] };
/** Who checks plans under each choice of judge, or null where nobody active holds it. */
export type Judges = {
	auto: Holder | null;
	architect: Holder | null;
	scrumMaster: Holder | null;
};

/** The team file, what the daemon works out for each agent, and the models the price table knows. */
export function useTeam() {
	const { data, again } = useQuery<{
		team: TeamFile;
		agents: Effective[];
		maxAgents: number;
		connectors?: ConnectorState[];
		kits?: RoleKit[];
		sandboxed?: boolean;
	}>("team.get", {});
	const { data: models } = useQuery<{ models: Model[] }>("models.list", {});
	return {
		team: data?.team,
		effective: data?.agents ?? [],
		/** Whether each custom connector runs on this computer (ADR 0030). */
		connectors: data?.connectors ?? [],
		/** What Farik offers each role on the team (ADR 0036). */
		kits: data?.kits ?? [],
		/** The most agents a team has that are not retired (SPEC F1), as the daemon says. */
		most: data?.maxAgents,
		/** Whether sessions run in Docker's sandbox, where no command of an agent reaches a connector's keys. */
		sandboxed: data?.sandboxed ?? false,
		models: models?.models ?? [],
		/** Reads the team again now, rather than on the next event. */
		again,
	};
}

/** Pause, resume or retire an agent through `agent_update`; answers the refusal, if any, in words. */
export function useStatus() {
	const { client } = useConnection();
	const [refusal, setRefusal] = useState<string>();
	const set = async (
		agent: Agent,
		status: "active" | "paused" | "retired",
	): Promise<boolean> => {
		if (!client) return false;
		setRefusal(undefined);
		try {
			const reply = await client.command({
				command: "agent_update",
				body: { agentId: agent.id, status },
			});
			if ("error" in reply) {
				setRefusal(
					commandSaid(reply.error.detail, {
						name: agent.displayName,
						role: roleName(agent.role),
					}),
				);
				return false;
			}
			return true;
		} catch {
			// The connection closed: the page shows that it is lost.
			return false;
		}
	};
	return { set, refusal };
}

/** The Team page: each agent on a card, with its model and a Pause or Resume. */
export function Team() {
	const { team, effective, most, again } = useTeam();
	const status = useStatus();
	const { data: activity } = useQuery<{
		activity: { agentId: string; line: string }[];
	}>("team.activity", {});
	const [open, setOpen] = useState<"save" | "use">();
	const [savedAs, setSavedAs] = useState<string>();
	if (!team) return null;
	const agents = team.agents.filter((a) => a.status !== "retired");
	const designer = agents.find((a) => a.role === "ui_ux_designer");
	const developer = agents.find((a) => a.role === "software_developer");
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("teamMembers")}</h1>
			<p>
				{t("teamPageLead")
					.replace("{count}", String(agents.length))
					.replace("{name}", team.name)}
			</p>
			<div className={styles.actions}>
				<Button onClick={() => setOpen("use")}>{t("templateUseOpen")}</Button>
				<Button onClick={() => setOpen("save")}>{t("templateSaveOpen")}</Button>
			</div>
			{savedAs && (
				<p role="status" className={styles.notice}>
					{t("savedAsBefore")}
					<strong>{savedAs}</strong>
					{t("savedAsAfter")}
					<Link to="/settings">{t("settings")}</Link>.
				</p>
			)}
			{open === "save" && (
				<SaveTemplate
					agents={agents}
					onSaved={(name) => {
						setSavedAs(name);
						setOpen(undefined);
					}}
					onClose={() => setOpen(undefined)}
				/>
			)}
			{open === "use" && (
				<UseTemplate
					current={team}
					onApplied={again}
					onClose={() => setOpen(undefined)}
				/>
			)}
			{status.refusal && (
				<p role="alert" className={styles.alert}>
					{status.refusal}
				</p>
			)}
			<ul className={styles.cards} aria-label={t("teamMembers")}>
				{agents.map((agent) => {
					const name = agent.displayName;
					const paused = agent.status === "paused";
					const avatar = AVATAR_URLS[agent.avatar as AvatarKey];
					return (
						<li key={agent.id} className={styles.card}>
							<div className={styles.cardHead}>
								{avatar && (
									<img
										className={styles.face}
										style={ringOf(agent.role)}
										src={avatar}
										alt=""
									/>
								)}
								<span>
									<strong>{name}</strong> <RoleTag role={agent.role} />
								</span>
							</div>
							<p>{agent.persona}</p>
							<p className={styles.muted}>
								{paused
									? t("agentPaused")
									: (activity?.activity.find((a) => a.agentId === agent.id)
											?.line ?? t("agentActive"))}
							</p>
							<dl className={styles.facts}>
								<dt>{t("agentModel")}</dt>
								<dd>
									{effective.find((e) => e.id === agent.id)?.model.label ??
										t("agentModelRole")}
								</dd>
							</dl>
							<div className={styles.actions}>
								<Link to={`/team/${agent.id}`}>
									{t("agentEditLink").replace("{name}", name)}
								</Link>
								<Button
									onClick={() =>
										status.set(agent, paused ? "active" : "paused")
									}
								>
									{t(paused ? "agentResume" : "agentPause").replace(
										"{name}",
										name,
									)}
								</Button>
							</div>
						</li>
					);
				})}
			</ul>
			<AddSomeone team={team} most={most ?? 0} />
			<section className={styles.section} aria-labelledby="changes-heading">
				<h2 id="changes-heading">{t("teamChangesTitle")}</h2>
				<p>{t("teamChangesBody")}</p>
			</section>
			<section className={styles.section} aria-labelledby="cost-heading">
				<h2 id="cost-heading">{t("teamCostTitle")}</h2>
				{designer && developer ? (
					<p>
						{t("teamCostDesigner", {
							designer: designer.displayName,
							developer: developer.displayName,
						})}{" "}
						{t("teamCostLimitBefore")}
						<Link to="/costs">{t("costs")}</Link>
						{t("teamCostLimitAfter")}
					</p>
				) : (
					<p>
						{t("firstDay")} {t("teamCostNote")}
					</p>
				)}
			</section>
		</div>
	);
}

/** "Add someone": a role, and a name from the spare names, saved as a change to the team. */
function AddSomeone({ team, most }: { team: TeamFile; most: number }) {
	const { client } = useConnection();
	const [role, setRole] = useState<Agent["role"]>("software_developer");
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const full = team.agents.filter((a) => a.status !== "retired").length >= most;
	const add = async () => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		const newcomer = someone(team.agents, {
			id: "",
			displayName: "",
			role,
			status: "active",
		});
		try {
			await client.call("team.save", {
				team: { ...team, agents: [...team.agents, newcomer] },
			});
		} catch (e) {
			setRefused(saidAll(e));
		}
		setBusy(false);
	};
	return (
		<section className={styles.section} aria-labelledby="add-heading">
			<h2 id="add-heading">{t("teamAdd")}</h2>
			<div className={styles.field}>
				<label htmlFor="add-role">{t("teamAddRole")}</label>
				<select
					id="add-role"
					className={styles.select}
					value={role}
					disabled={full}
					onChange={(e) => setRole(e.target.value as Agent["role"])}
				>
					{ROLES.map((r) => (
						<option key={r} value={r}>
							{roleName(r)}
						</option>
					))}
				</select>
			</div>
			{full && <p className={styles.muted}>{t("teamFull")}</p>}
			{refused && (
				<p role="alert" className={styles.alert}>
					{refused}
				</p>
			)}
			<span>
				<Button busy={busy} disabled={full} onClick={add}>
					{t("teamAdd")}
				</Button>
			</span>
		</section>
	);
}
