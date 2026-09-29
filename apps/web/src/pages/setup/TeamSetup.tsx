import type { Role } from "@farik/ui";
import { useState } from "react";
import { Outlet, useNavigate, useOutletContext } from "react-router";
import { useConnection } from "../../app/connection.tsx";
import { useQuery } from "../../app/store.ts";
import { t } from "../../strings/t.ts";
import { slug } from "./SetupProject.tsx";

export type Agent = {
	id: string;
	displayName: string;
	role: Role;
	avatar?: string;
	persona?: string;
	status: string;
	model?: unknown;
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
		[key: string]: unknown;
	};
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
	/** Every suggested or added agent, and whether it is on the team. */
	members: { agent: Agent; on: boolean }[];
	criteria: Library;
	/** The two permission questions, unanswered until the user answers them. */
	answers: { commands?: boolean; push?: boolean };
};
type Setup = { draft: Draft; change: (next: Draft) => void };

/** The names "Add someone" suggests, in order, each with its picture. */
const SPARE = ["Noor", "Ivo", "Lena", "Sami", "Rui"];

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
	return { ...draft.team, agents };
}

/** A second agent of `role`, named from the spare names nobody uses yet. */
export function someone(draft: Draft, like: Agent): Agent {
	const used = new Set(draft.members.map((m) => m.agent.displayName));
	const index = SPARE.findIndex((name) => !used.has(name));
	const name =
		SPARE[index] ?? `${like.displayName} ${draft.members.length + 1}`;
	return {
		...like,
		id: slug(name),
		displayName: name,
		avatar: `extra-${(index < 0 ? 0 : index) + 1}`,
	};
}

/** The layout of the team screens: the suggested team, fetched once, held for every screen. */
export function TeamSetup() {
	const { data } = useQuery<{ team: Team; criteria: Library }>(
		"team.propose",
		{},
	);
	const [mine, setMine] = useState<Draft>();
	const draft: Draft | undefined =
		mine ??
		(data && {
			team: data.team,
			members: data.team.agents.map((agent) => ({ agent, on: true })),
			criteria: data.criteria,
			answers: {},
		});
	if (!draft) return null;
	return <Outlet context={{ draft, change: setMine } satisfies Setup} />;
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
			setRefused((e as Error).message);
			setBusy(false);
		}
	};
	return { start, busy, refused };
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
			} as const
		)[role],
	);
}
