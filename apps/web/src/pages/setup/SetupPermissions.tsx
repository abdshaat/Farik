import { Button, Choice } from "@farik/ui";
import { useNavigate } from "react-router";
import { useQuery } from "../../app/store.ts";
import type { en } from "../../strings/en.ts";
import { t } from "../../strings/t.ts";
import type { Tier } from "../Team.tsx";
import styles from "./setup.module.css";
import {
	type Agent,
	type Draft,
	PutBack,
	roleName,
	teamOf,
	useDefaults,
	useSetup,
} from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

type Answer = "" | "yes" | "no";
const word = (on: boolean | undefined): Answer =>
	on === undefined ? "" : on ? "yes" : "no";

/** Each tier, in the order it is said, and what it lets an agent do, after its name. */
const MAY: [Tier, keyof typeof en][] = [
	["read", "mayRead"],
	["write_workspace", "mayWrite"],
	["execute", "mayExecute"],
	["git_local", "mayGitLocal"],
	["network", "mayNetwork"],
	["git_remote", "mayGitRemote"],
	["external_effect", "mayExternal"],
];

/** What an agent holding `tiers` may do, in one sentence: "Reads the project and …". */
export function mayOf(tiers: Tier[]): string {
	const said = MAY.filter(([tier]) => tiers.includes(tier)).map(([, key]) =>
		t(key),
	);
	const last = said.pop() ?? "";
	const line = said.length ? `${said.join(", ")} and ${last}` : last;
	return `${line.charAt(0).toUpperCase()}${line.slice(1)}.`;
}

/** The two permission questions, their notes, and "Put back the default": setup's and Settings'. */
export function PermissionChoices({
	commands,
	push,
	developers,
	onAnswer,
	putBack,
}: {
	commands: boolean | undefined;
	push: boolean | undefined;
	developers: Agent[];
	onAnswer: (next: Draft["answers"]) => void;
	putBack: (() => void) | undefined;
}) {
	return (
		<>
			<div className={styles.card}>
				<Choice<Answer>
					name="commands"
					legend={t("mayCommands")}
					value={word(commands)}
					onChange={(v) => onAnswer({ commands: v === "yes" })}
					options={[
						{
							value: "yes",
							label: t("mayCommandsYes"),
							description: t("mayCommandsYesNote"),
						},
						{
							value: "no",
							label: t("mayCommandsNo"),
							description: t("mayCommandsNoNote"),
						},
					]}
				/>
				<p>{t("mayCommandsNote")}</p>
				{commands === false && (
					<p className={styles.warn}>{t("mayStillChecks")}</p>
				)}
			</div>
			<div className={styles.card}>
				<Choice<Answer>
					name="push"
					legend={t("mayPush")}
					value={word(push)}
					onChange={(v) => onAnswer({ push: v === "yes" })}
					options={[
						{
							value: "yes",
							label: t("mayPushYes"),
							description: t("mayPushYesNote"),
						},
						{
							value: "no",
							label: t("mayPushNo"),
							description: t("mayPushNoNote"),
						},
					]}
				/>
				<p>
					{developers.length === 1
						? t("mayPushNoteOne").replace(
								"{name}",
								developers[0]?.displayName ?? "",
							)
						: t("mayPushNoteMany")}
				</p>
			</div>
			<PutBack onClick={putBack} />
		</>
	);
}

/** Setup's sixth step: the two questions Farik asks before anything runs. */
export function SetupPermissions() {
	const navigate = useNavigate();
	const { draft, change } = useSetup();
	const defaults = useDefaults()?.policy.permissions;
	const { commands, push } = draft.answers;
	const answered = commands !== undefined && push !== undefined;
	const team = teamOf(draft);
	const agents = team.agents;
	const developers = agents.filter((a) => a.role === "software_developer");
	// What each agent may do is the daemon's answer for the team as the answers leave it.
	const { data: checked } = useQuery<{
		agents?: { id: string; tiers: Tier[] }[];
	}>("team.validate", { team });
	const tiersOf = (id: string) =>
		checked?.agents?.find((a) => a.id === id)?.tiers;

	const answer = (next: Draft["answers"]) => {
		const answers = { ...draft.answers, ...next };
		const permissions = { ...draft.team.policy.permissions };
		if (answers.commands !== undefined)
			permissions.runCommands = answers.commands;
		if (answers.push !== undefined) permissions.push = answers.push;
		change({
			...draft,
			answers,
			team: { ...draft.team, policy: { ...draft.team.policy, permissions } },
		});
	};

	return (
		<Wizard step={5} title={t("mayTitle")} lead={t("mayLead")}>
			<PermissionChoices
				commands={commands}
				push={push}
				developers={developers}
				onAnswer={answer}
				putBack={
					defaults &&
					(() =>
						answer({
							commands: defaults.runCommands,
							push: defaults.push,
						}))
				}
			/>
			<section className={styles.card} aria-labelledby="may-already">
				<h2 id="may-already" className={styles.heading}>
					{t("mayAlready")}
				</h2>
				<dl className={styles.facts}>
					{agents.map((agent) => (
						<div key={agent.id}>
							<dt>
								{agent.displayName}
								<small>{roleName(agent.role)}</small>
							</dt>
							<dd>{tiersOf(agent.id) ? mayOf(tiersOf(agent.id) ?? []) : ""}</dd>
						</div>
					))}
				</dl>
			</section>
			<div className={styles.foot}>
				<Button onClick={() => navigate("/setup/team")}>{t("back")}</Button>
				<span>
					{!answered && (
						<span className={styles.note}>{t("mayAnswerBoth")}</span>
					)}
					<Button
						kind="primary"
						disabled={!answered}
						onClick={() => navigate("/setup/spending")}
					>
						{t("continue")}
					</Button>
				</span>
			</div>
		</Wizard>
	);
}
