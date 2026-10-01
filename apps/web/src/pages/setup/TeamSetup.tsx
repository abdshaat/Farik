import { Button, type Role } from "@farik/ui";
import { type CSSProperties, useState } from "react";
import { Outlet, useNavigate, useOutletContext } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { saidAll } from "../../app/refusals.ts";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import type { Template } from "../SavedTeams.tsx";
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
	model?: unknown;
	/** The connectors its sessions get (spec 5.6). */
	mcpServers?: { name: string; source: string }[];
};
export type Judgment = {
	required?: "always" | "never";
	questions?: string[];
	judge?: "auto" | "architect" | "scrum_master";
};
export type Team = {
	name: string;
	agents: Agent[];
	budgets: { dailyUsd?: number; [key: string]: unknown };
	policy: {
		integration: string;
		judgment?: Judgment;
		permissions?: { runCommands?: boolean; push?: boolean };
		[key: string]: unknown;
	};
	rules: {
		requireNewTests?: boolean;
		maxTaskBudgetUsd?: number;
		uiPaths?: string[];
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
	/** Every suggested or added agent, whether it is on the team, and its row's own key. */
	members: { agent: Agent; on: boolean; key: number }[];
	criteria: Library;
	/** The two permission questions, unanswered until the user answers them. */
	answers: { commands?: boolean; push?: boolean };
	/** How to open the app, asked on the Designer's row. */
	preview: PreviewForm;
	/** Where the team started from: the suggested team, a saved team, or from scratch. */
	start: Start;
	/** The saved team's name, when it started from one; its permission answers are not asked again. */
	from?: string;
};
export type Start = "suggested" | "saved" | "scratch";
/** What `team.propose` answers: the suggested team and checks, and the agents this computer cannot run. */
export type Proposed = {
	team: Team;
	criteria: Library;
	unavailable?: { agentId: string }[];
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
/** The pictures added agents draw from: extra-1 is Iris's and extra-4 the Finance Specialist's (F10). */
const EXTRAS = ["extra-2", "extra-3", "extra-5"];

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
	return { ...draft.team, agents, ...(preview && { preview }) };
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
		// Once all three are taken, they are shared again (the team holds seven at most).
		avatar:
			EXTRAS.find((key) => !agents.some((a) => a.avatar === key)) ??
			`extra-${[2, 3, 5][agents.length % 3]}`,
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
 */
export function draftOf(
	proposed: Proposed,
	start: Start,
	template?: Template,
): Draft {
	const unavailable = unavailableRoles(proposed);
	const suggested = proposed.team.agents;
	const role = (r: Agent["role"]) => suggested.find((a) => a.role === r);
	const base = {
		team: proposed.team,
		criteria: proposed.criteria,
		answers: {},
		preview: formOf(proposed.team.preview),
		start,
	};
	if (start === "saved" && template) {
		const { permissions, judgment, integration } = template.policy;
		const { dailyUsd: _, ...budgets } = proposed.team.budgets;
		return {
			...base,
			from: template.name,
			team: {
				...proposed.team,
				policy: { ...proposed.team.policy, permissions, judgment, integration },
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
			})),
		};
	}
	if (start === "scratch")
		return {
			...base,
			members: (["product_manager", "software_developer"] as const).flatMap(
				(r, key) => {
					const agent = role(r);
					return agent
						? [{ agent: { ...agent, displayName: "" }, on: true, key }]
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
		})),
	};
}

/** The layout of the team screens: the suggested team, fetched once, held for every screen. */
export function TeamSetup() {
	const { data, again } = useQuery<Proposed>("team.propose", {});
	const [mine, setMine] = useState<Draft>();
	const unavailable = data ? unavailableRoles(data) : [];
	const draft: Draft | undefined = mine ?? (data && draftOf(data, "suggested"));
	if (!draft || !data) return null;
	return (
		<Outlet
			context={
				{
					draft,
					proposed: data,
					change: setMine,
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

/** "Start the team": the draft saved, the marker gone, the team resumed; then home. */
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
			navigate("/", { replace: true });
		} catch (e) {
			setRefused(saidAll(e));
			setBusy(false);
		}
	};
	return { start, busy, refused };
}

/** What putting a setting back puts back (SPEC 10): the values `farik init` writes. */
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
			} as const
		)[role],
	);
}

/** A face's ring in its role's tag colour, as the mockups draw it: the CSS reads `--ring`. */
export function ringOf(role: Role): CSSProperties {
	const token = role === "software_developer" ? "developer" : role;
	return {
		"--ring": `var(--farik-color-role-${token.replaceAll("_", "-")})`,
	} as CSSProperties;
}
