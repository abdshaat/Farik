import { Button, type Role } from "@catervas/ui";
import { type CSSProperties, useState } from "react";
import { Outlet, useNavigate, useOutletContext } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { saidAll } from "../../app/refusals.ts";
import { type ServeStatus, useQuery } from "../../app/store.ts";
import type { Effort } from "../../components/ModelFields.tsx";
import { t } from "../../strings/t.ts";
import type { Template } from "../SavedTeams.tsx";
import type { Tier } from "../Team.tsx";
import {
	formOf,
	type Preview,
	type PreviewForm,
	previewOf,
} from "./PreviewFields.tsx";
import { slug } from "./SetupProject.tsx";

export type Agent = {
	id: string;
	displayName: string;
	role: Role;
	avatar?: string;
	persona?: string;
	status: string;
	/** Its own model and effort; either left out is its role's. */
	model?: { id?: string; effort?: Effort } | undefined;
	/** The connectors its sessions get (spec 5.6). */
	mcpServers?: McpServer[];
	/** The permission tiers its role lacks and the team gives it. */
	grants?: Tier[] | undefined;
};
/** An agent's connector as `team.yaml` holds it: a built-in by name, or the user's own (ADR 0030). */
export type McpServer = {
	name: string;
	source: string;
	transport?: "stdio" | "http";
	command?: string;
	args?: string[];
	url?: string;
	headers?: Record<string, string>;
	credentialKeys?: string[];
	/** Present when the user signs in to the service instead of giving a key (ADR 0033). */
	oauth?: { clientId?: string; callbackPort?: number; scopes?: string[] };
	tools?: Record<string, "network" | "paid" | "external_effect" | "denied">;
};
export type Team = {
	name: string;
	agents: Agent[];
	budgets: { dailyUsd?: number; [key: string]: unknown };
	policy: {
		integration: string;
		permissions?: { runCommands?: boolean; push?: boolean };
		/** The format new documents are written in; absent means HTML. */
		documentFormat?: "html" | "markdown";
		/** The branch finished work goes into; absent means the repository's main branch. */
		integrationBranch?: string;
		/** One agent working alone, outside any team function (ADR 0071). */
		solo?: boolean;
		/** Whether the team's documents are kept on this computer, out of git (ADR 0074). */
		documentsLocal?: boolean;
		/** The remotes a task branch may be pushed to; absent means `origin`. */
		pushRemotes?: string[];
		[key: string]: unknown;
	};
	rules: {
		requireNewTests?: boolean;
		maxTaskBudgetUsd?: number;
		uiPaths?: string[];
		/** Which of the five shipped protected globs no longer apply (ADR 0080). */
		liftedPaths?: string[];
		/** Globs an agent may read and no tool may write. */
		readOnlyPaths?: string[];
		/** The owner's own off-limits globs, on top of the shipped ones. */
		protectedPaths?: string[];
		/** Patterns a command must not match. */
		forbiddenCommands?: string[];
		/** Git subcommands a command may run on top of the read-only eight. */
		gitSubcommands?: string[];
		[key: string]: unknown;
	};
	preview?: Preview;
};
export type Criterion = {
	name: string;
	text: string;
	source?: string;
	verification: { method: string; command?: string; rubric?: string[] };
};
export type Library = { criteria: Criterion[] };

/** What the wizard's screens 5 to 8 build, kept until "Start the team" sends it. */
export type Draft = {
	team: Team;
	/**
	 * Every suggested or added agent, whether it is on the team, its row's own key, and whether it
	 * is a role Catervas does not suggest, offered under "More roles".
	 */
	members: {
		agent: Agent;
		on: boolean;
		key: number;
		more: boolean;
		/** Added on the team step with "Add someone": shown even while it is off the team. */
		added?: boolean;
	}[];
	criteria: Library;
	/** The two permission questions, unanswered until the user answers them. */
	answers: { commands?: boolean; push?: boolean };
	/** How to open the app, asked on the Designer's row. */
	preview: PreviewForm;
	/** Where the team started from: the suggested team, a ready-made or saved team, or from scratch. */
	start: Start;
	/** The saved team's name, when it started from one; its permission answers are not asked again. */
	from?: string;
};
export type Start = "suggested" | "builtin" | "saved" | "scratch" | "solo";
/** Why a ready-made team fits the folder: what the scan found most of. */
export type SuggestedTeam = {
	slug: string;
	signal: "code" | "money" | "shop" | "new";
	language: string | null;
	tests: boolean;
};
/** A role's own model, effort and persona, as Catervas ships it. */
export type RoleDefault = { model: string; effort: Effort; persona: string };
/** What `team.propose` answers: the suggested team and checks, and the agents this computer cannot run. */
export type Proposed = {
	team: Team;
	criteria: Library;
	unavailable?: { agentId: string }[];
	/** Every ready-made team, the one the folder fits best first. */
	suggestedTeams?: SuggestedTeam[];
	roleDefaults?: Partial<Record<Role, RoleDefault>>;
};
type Setup = {
	draft: Draft;
	/** The suggested team, which every start builds from. */
	proposed: Proposed;
	change: (next: Draft) => void;
	/**
	 * The roles this computer cannot run, whose rows start unticked and cannot be ticked (step 12,
	 * D3): by role, so a saved team's Designer is held off whatever its id.
	 */
	unavailable: Agent["role"][];
	/** Asks the daemon again, after the user installs Docker. */
	checkAgain: () => void;
};

/** The names "Add someone" suggests, in order, each with its picture. */
const SPARE = ["Noor", "Ivo", "Lena", "Sami", "Rui"];
/**
 * The pictures added agents draw from: extra-1 is Iris's, extra-4 the Finance Specialist's and
 * extra-5 the Procurement Specialist's (F10).
 */
const EXTRAS = ["extra-2", "extra-3"];
/** The roles Catervas does not suggest, offered under "More roles" in the order the Team page lists them. */
const MORE_ROLES = ["finance_specialist", "procurement_specialist"] as const;

/** The team the draft stands for: the agents on it, each with an id from its name. */
export function teamOf(draft: Draft): Team {
	const taken = new Set<string>();
	const agents = draft.members
		.filter((m) => m.on)
		.map(({ agent }) => {
			const base = slug(agent.displayName) || agent.role.replaceAll("_", "-");
			let id = base;
			for (let n = 2; taken.has(id); n++) id = `${base}-${n}`;
			taken.add(id);
			return { ...agent, id };
		});
	const preview =
		agents.some((a) => a.role === "ui_ux_designer") && previewOf(draft.preview);
	// The daemon may still answer `planInSprints`; the setup never sends it back.
	const { planInSprints: _, solo: __, ...rest } = draft.team.policy;
	const policy = draft.start === "solo" ? { ...rest, solo: true } : rest;
	return { ...draft.team, policy, agents, ...(preview && { preview }) };
}

/** Another agent like `like`, named from the spare names none of `agents` uses yet. */
export function someone(agents: Agent[], like: Agent): Agent {
	const used = new Set(agents.map((a) => a.displayName));
	const index = SPARE.findIndex((name) => !used.has(name));
	const name = SPARE[index] ?? `${like.displayName} ${agents.length + 1}`;
	return {
		...like,
		id: slug(name),
		displayName: name,
		// The Finance Specialist and the Procurement Specialist have their own pictures. Once both
		// extras are taken, they are shared again (the team holds seven at most).
		avatar:
			like.role === "finance_specialist"
				? "finance-specialist"
				: like.role === "procurement_specialist"
					? "extra-5"
					: (EXTRAS.find((key) => !agents.some((a) => a.avatar === key)) ??
						`extra-${[2, 3][agents.length % 2]}`),
	};
}

/** The roles `team.propose` names unavailable on this computer, from the suggested agents' ids. */
export function unavailableRoles(proposed: Proposed): Agent["role"][] {
	const ids = (proposed.unavailable ?? []).map((u) => u.agentId);
	return proposed.team.agents
		.filter((a) => ids.includes(a.id))
		.map((a) => a.role);
}

/**
 * The draft a start makes from the suggested team: its six; a saved team's agents, each field the
 * template leaves out the role's, with its four answers; or a Product Manager and a Developer,
 * unnamed. A saved team never goes through `template.preview`: setup replaces the starter team.
 * After them, whatever the start, a row for each role Catervas does not suggest that none of them
 * holds, unticked, to be offered under "More roles", each named from the spare names the rows
 * above it leave.
 */
export function draftOf(
	proposed: Proposed,
	start: Start,
	template?: Template,
	soloRole: Role = "software_developer",
): Draft {
	if (start === "solo") return soloDraftOf(proposed, soloRole);
	const draft = teamDraftOf(proposed, start, template);
	const members = [...draft.members];
	for (const role of MORE_ROLES) {
		if (members.some((m) => m.agent.role === role)) continue;
		const agent = someone(
			members.map((m) => m.agent),
			{ id: "", displayName: "", role, status: "active" },
		);
		members.push({ agent, on: false, key: members.length, more: true });
	}
	return { ...draft, members };
}

/** An agent of `role` as Catervas ships it: the suggested one, else the role's defaults. */
export function agentOfRole(
	proposed: Proposed,
	role: Role,
	agents: Agent[],
): Agent {
	const suggested = proposed.team.agents.find((a) => a.role === role);
	const shipped = proposed.roleDefaults?.[role];
	const like: Agent = suggested ?? {
		id: "",
		displayName: "",
		role,
		status: "active",
		...(shipped && {
			persona: shipped.persona,
			model: { id: shipped.model, effort: shipped.effort },
		}),
	};
	// The suggested agent keeps its own name and picture unless the team already has them.
	return suggested &&
		!agents.some((a) => a.displayName === suggested.displayName)
		? suggested
		: {
				...someone(agents, like),
				...(like.persona !== undefined && { persona: like.persona }),
				...(like.model && { model: like.model }),
			};
}

/** One agent of `role`, alone: the suggested one's name and picture, or a spare one. */
function soloDraftOf(proposed: Proposed, role: Role): Draft {
	return {
		team: proposed.team,
		criteria: proposed.criteria,
		answers: {},
		preview: formOf(proposed.team.preview),
		start: "solo",
		members: [
			{ agent: agentOfRole(proposed, role, []), on: true, key: 0, more: false },
		],
	};
}

/** `draftOf`'s team rows, before "More roles". */
function teamDraftOf(
	proposed: Proposed,
	start: Start,
	template?: Template,
): Draft {
	const unavailable = unavailableRoles(proposed);
	const suggested = proposed.team.agents;
	const role = (r: Agent["role"]) => {
		const shipped = proposed.roleDefaults?.[r];
		return (
			suggested.find((a) => a.role === r) ??
			(shipped && {
				persona: shipped.persona,
				model: { id: shipped.model, effort: shipped.effort },
			})
		);
	};
	const base = {
		team: proposed.team,
		criteria: proposed.criteria,
		answers: {},
		preview: formOf(proposed.team.preview),
		start,
	};
	if ((start === "saved" || start === "builtin") && template) {
		const { permissions, integration, documentFormat } = template.policy;
		const { dailyUsd: _, ...budgets } = proposed.team.budgets;
		return {
			...base,
			from: template.name,
			team: {
				...proposed.team,
				policy: {
					...proposed.team.policy,
					permissions,
					integration,
					...(documentFormat !== undefined && { documentFormat }),
				},
				budgets:
					template.budgets.dailyUsd === undefined
						? budgets
						: { ...budgets, dailyUsd: template.budgets.dailyUsd },
			},
			answers: { commands: permissions.runCommands, push: permissions.push },
			members: template.agents.map((agent, key) => ({
				agent: { ...role(agent.role), ...agent, status: "active" },
				on: !unavailable.includes(agent.role),
				key,
				more: false,
			})),
		};
	}
	if (start === "scratch")
		return {
			...base,
			members: (["product_manager", "software_developer"] as const).flatMap(
				(r, key) => {
					const agent = suggested.find((a) => a.role === r);
					return agent
						? [
								{
									agent: { ...agent, displayName: "" },
									on: true,
									key,
									more: false,
								},
							]
						: [];
				},
			),
		};
	return {
		...base,
		members: suggested.map((agent, key) => ({
			agent,
			on: !unavailable.includes(agent.role),
			key,
			more: false,
		})),
	};
}

const DRAFT = "catervas.setupDraft";

// The team's answers, kept for this tab while the project is the same, so going back to the scan
// and forward again does not lose them. Storage can refuse (a private window): nothing is kept.
export function keepDraft(root: string, draft: Draft): void {
	try {
		sessionStorage.setItem(DRAFT, JSON.stringify({ root, draft }));
	} catch {}
}

export function keptDraft(root: string): Draft | undefined {
	try {
		const kept = JSON.parse(sessionStorage.getItem(DRAFT) ?? "null") as {
			root: string;
			draft: Draft;
		} | null;
		const d = kept?.root === root ? kept.draft : undefined;
		return d && Array.isArray(d.members) && typeof d.preview?.start === "string"
			? d
			: undefined;
	} catch {
		return undefined;
	}
}

export function forgetDraft(): void {
	try {
		sessionStorage.removeItem(DRAFT);
	} catch {}
}

/** The layout of the team screens: the suggested team, fetched once, held for every screen. */
export function TeamSetup() {
	const { data, again } = useQuery<Proposed>("team.propose", {});
	const { data: serve } = useQuery<ServeStatus>("serve.status", {});
	const [mine, setMine] = useState<Draft>();
	const unavailable = data ? unavailableRoles(data) : [];
	const root = serve?.projectRoot ?? "";
	const draft: Draft | undefined =
		mine ?? (data && serve && (keptDraft(root) ?? draftOf(data, "suggested")));
	if (!draft || !data) return null;
	return (
		<Outlet
			context={
				{
					draft,
					proposed: data,
					change: (next: Draft) => {
						setMine(next);
						keepDraft(root, next);
					},
					unavailable,
					checkAgain: again,
				} satisfies Setup
			}
		/>
	);
}

export function useSetup(): Setup {
	return useOutletContext<Setup>();
}

/** "Start the team": the draft saved, the marker gone, the team resumed; then the offer, which goes home when it has none. */
export function useStart() {
	const { client } = useConnection();
	const navigate = useNavigate();
	const { draft } = useSetup();
	const [busy, setBusy] = useState(false);
	const [refused, setRefused] = useState<string>();
	const start = async () => {
		if (!client) return;
		setBusy(true);
		setRefused(undefined);
		try {
			await client.call("team.start", {
				team: teamOf(draft),
				criteria: draft.criteria,
			});
			forgetDraft();
			navigate("/setup/ready", { replace: true });
		} catch (e) {
			setRefused(saidAll(e));
			setBusy(false);
		}
	};
	return { start, busy, refused };
}

/** What putting a setting back puts back (SPEC 10): the values `catervas init` writes. */
export type Defaults = {
	budgets: Team["budgets"];
	/** The permission answers always come with the defaults, so the page keeps no fallback of its own. */
	policy: Team["policy"] & {
		permissions: { runCommands: boolean; push: boolean };
	};
	rules: Team["rules"];
	/** The globs that make a Developer's change a UI change (spec 5.12). */
	uiPaths: string[];
};

/** The defaults, once the daemon has answered them. */
export function useDefaults(): Defaults | undefined {
	return useQuery<Defaults>("settings.defaults", {}).data;
}

/** "Put back the default", beside the setting it puts back. */
export function PutBack({ onClick }: { onClick?: (() => void) | undefined }) {
	return (
		<span>
			<Button kind="quiet" disabled={!onClick} onClick={() => onClick?.()}>
				{t("putBack")}
			</Button>
		</span>
	);
}

/** Each role's name as the setup screens say it. */
export function roleName(role: Role): string {
	return t(
		(
			{
				product_manager: "roleProductManager",
				scrum_master: "roleScrumMaster",
				architect: "roleArchitect",
				software_developer: "roleDeveloper",
				marketing_specialist: "roleMarketing",
				ui_ux_designer: "roleDesigner",
				finance_specialist: "roleFinance",
				procurement_specialist: "roleProcurement",
			} as const
		)[role],
	);
}

/** A face's ring in its role's tag colour, as the mockups draw it: the CSS reads `--ring`. */
export function ringOf(role: Role): CSSProperties {
	const token = role === "software_developer" ? "developer" : role;
	return {
		"--ring": `var(--catervas-color-role-${token.replaceAll("_", "-")})`,
	} as CSSProperties;
}
