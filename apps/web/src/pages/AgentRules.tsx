import { Button, Segmented } from "@catervas/ui";
import { t } from "../strings/t.ts";
import type { Tag } from "./ConnectorAdd.tsx";
import {
	CommandList,
	GitList,
	OwnFiles,
	SessionFields,
	SHIPPED,
} from "./GovernorRules.tsx";
import styles from "./pages.module.css";
import setup from "./setup/setup.module.css";
import type { Agent, AgentRulesFile, Team } from "./setup/TeamSetup.tsx";

/** The agent with `patch` laid over its own rules; a list left empty is left out, and so is `rules`. */
function withAgentRules<A extends Agent>(agent: A, patch: AgentRulesFile): A {
	const rules: AgentRulesFile = { ...agent.rules, ...patch };
	for (const key of Object.keys(rules) as (keyof AgentRulesFile)[])
		if ((rules[key] ?? []).length === 0) delete rules[key];
	const { rules: _, ...rest } = agent;
	return (Object.keys(rules).length > 0 ? { ...rest, rules } : rest) as A;
}

/** One file the team keeps out of reach, as the agent's page lists it. */
type TeamRow = { glob: string; kind: "off" | "read"; name?: string };

/** The files the team's rules keep out of reach: the shipped globs it has not lifted, then its own. */
function teamRows(team: Team): TeamRow[] {
	const lifted = team.rules.liftedPaths ?? [];
	const shipped = SHIPPED.filter((s) => !lifted.includes(s.glob)).map((s) => ({
		glob: s.glob,
		kind: "off" as const,
		name: t(s.name),
	}));
	const rows = [
		...shipped,
		...(team.rules.protectedPaths ?? []).map((glob) => ({
			glob,
			kind: "off" as const,
		})),
		...(team.rules.readOnlyPaths ?? []).map((glob) => ({
			glob,
			kind: "read" as const,
		})),
	];
	return rows.filter((r, i) => rows.findIndex((x) => x.glob === r.glob) === i);
}

/**
 * "Rules for <name>" (ADR 0080): the team's rules as they reach this agent, a way to lift one for
 * it alone, and the files, commands, git and session limits only this agent follows. The page's
 * own Save sends them with the rest of the agent.
 */
export function AgentRules({
	name,
	team,
	agent,
	onChange,
}: {
	name: string;
	team: Team;
	agent: Agent;
	onChange: (next: Agent) => void;
}) {
	const lifted = agent.rules?.liftedPaths ?? [];
	const rows = teamRows(team);
	const set = (patch: AgentRulesFile) => onChange(withAgentRules(agent, patch));
	return (
		<section className={styles.section} aria-labelledby="agent-rules-heading">
			<h2 id="agent-rules-heading">{t("agentRulesTitle", { name })}</h2>
			<p className={styles.muted}>{t("agentRulesLead", { name })}</p>
			<h3 className={styles.subheading}>{t("agentRulesFiles")}</h3>
			{rows.length === 0 ? (
				<p className={styles.muted}>{t("agentRulesFilesNone")}</p>
			) : (
				<ul className={styles.globs}>
					{rows.map(({ glob, kind, name: plain }) => {
						const off = lifted.includes(glob);
						return (
							<li key={glob}>
								<code>{glob}</code>
								<span className={styles.muted}>
									{plain ? `${plain}, ` : ""}
									{t(kind === "off" ? "agentRulesOff" : "agentRulesRead")}
								</span>
								{off ? (
									<>
										<span className={styles.warn}>
											{t("agentLifted", { name })}
										</span>
										<Button
											kind="quiet"
											onClick={() =>
												set({ liftedPaths: lifted.filter((g) => g !== glob) })
											}
										>
											{t("agentLiftUndo", { name })}{" "}
											<span className={setup.hidden}>{glob}</span>
										</Button>
									</>
								) : (
									<Button
										kind="quiet"
										onClick={() => set({ liftedPaths: [...lifted, glob] })}
									>
										{t("agentLift", { name })}{" "}
										<span className={setup.hidden}>{glob}</span>
									</Button>
								)}
							</li>
						);
					})}
				</ul>
			)}
			<OwnFiles
				idPrefix="agent-files"
				protectedPaths={agent.rules?.protectedPaths}
				readOnlyPaths={agent.rules?.readOnlyPaths}
				onChange={set}
			/>
			<h3 className={styles.subheading}>{t("agentRulesCommands", { name })}</h3>
			<CommandList
				idPrefix="agent-command"
				raw={false}
				patterns={agent.rules?.forbiddenCommands ?? []}
				onChange={(forbiddenCommands) => set({ forbiddenCommands })}
			/>
			<h3 className={styles.subheading}>{t("agentRulesGit", { name })}</h3>
			<GitList
				idPrefix="agent-git"
				listed={agent.rules?.gitSubcommands ?? []}
				always={false}
				onChange={(gitSubcommands) => set({ gitSubcommands })}
			/>
			<h3 className={styles.subheading}>{t("agentRulesSession", { name })}</h3>
			<SessionFields
				idPrefix="agent-session"
				value={agent.session}
				onChange={(session) => {
					const { session: _, ...rest } = agent;
					onChange(session ? { ...rest, session } : rest);
				}}
			/>
		</section>
	);
}

type Answer = "runs" | "ask" | "never";

/** What a tool does, as the owner chooses it: "Runs", "Asks first" or "Never" (ADR 0080, decision 10). */
export function ToolChoice({
	tool,
	words,
	label,
	asAdded,
	onChange,
}: {
	tool: string;
	/** The tool in the user's words; the tool's name when the service gave none. */
	words: string;
	/** What the team file holds now. */
	label: Tag;
	/** The label it had when the page was read; "Runs" and "Asks first" go back to it. */
	asAdded: Tag;
	onChange: (label: Tag) => void;
}) {
	// A paid tool never runs without the owner (ADR 0041): it asks first or it never runs.
	const paid = asAdded === "paid";
	const now: Answer =
		label === "denied"
			? "never"
			: label === "ask" || label === "paid"
				? "ask"
				: "runs";
	const options: { value: Answer; label: string }[] = [
		...(paid ? [] : [{ value: "runs" as const, label: t("toolRuns") }]),
		{ value: "ask", label: t("toolAsks") },
		{ value: "never", label: t("toolNever") },
	];
	return (
		<Segmented
			name={`tool-${tool}`}
			legend={words}
			options={options}
			value={now}
			onChange={(to) =>
				onChange(
					to === "never"
						? "denied"
						: to === "ask"
							? paid
								? "paid"
								: "ask"
							: asAdded === "network"
								? "network"
								: "external_effect",
				)
			}
		/>
	);
}

/**
 * A connector's tools, each with its choice, behind "What each tool does". A tool the owner's plan
 * marks (ADR 0042) is the plan's to run and shows no choice.
 */
export function ConnectorTools({
	name,
	tools,
	savedTools,
	words,
	planMarked,
	onChange,
}: {
	name: string;
	tools: Record<string, Tag> | undefined;
	savedTools: Record<string, Tag> | undefined;
	words: Record<string, string>;
	planMarked: string[];
	onChange: (tools: Record<string, Tag>) => void;
}) {
	const shown = Object.entries(tools ?? {}).filter(
		([tool]) => !planMarked.includes(tool),
	);
	if (shown.length === 0) return null;
	return (
		<details className={styles.toolChoices}>
			<summary>{t("toolsSummary")}</summary>
			<p className={styles.muted}>{t("toolsLead", { name })}</p>
			{shown.map(([tool, label]) => (
				<ToolChoice
					key={tool}
					tool={tool}
					words={words[tool] ?? tool}
					label={label}
					asAdded={savedTools?.[tool] ?? label}
					onChange={(next) => onChange({ ...tools, [tool]: next })}
				/>
			))}
		</details>
	);
}
