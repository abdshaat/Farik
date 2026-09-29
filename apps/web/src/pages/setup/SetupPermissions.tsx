import { Button, Choice } from "@farik/ui";
import { useNavigate } from "react-router";
import { t } from "../../strings/t.ts";
import styles from "./setup.module.css";
import { type Agent, type Draft, roleName, useSetup } from "./TeamSetup.tsx";
import { Wizard } from "./Wizard.tsx";

type Answer = "" | "yes" | "no";
const word = (on: boolean | undefined): Answer =>
	on === undefined ? "" : on ? "yes" : "no";

/** What `agent` may do with the answers given so far, in the setup's words. */
function may(agent: Agent, draft: Draft): string {
	const commands = draft.answers.commands !== false;
	switch (agent.role) {
		case "product_manager":
			return t("mayProductManager");
		case "scrum_master":
			return t("mayScrumMaster");
		case "architect":
			return t(commands ? "mayArchitect" : "mayArchitectNoCommands");
		case "software_developer": {
			const line = t(commands ? "mayDeveloper" : "mayDeveloperNoCommands");
			return draft.answers.push ? `${line} ${t("mayDeveloperPush")}` : line;
		}
		default:
			return t("mayMarketing");
	}
}

/** Setup's sixth step: the two questions Farik asks before anything runs. */
export function SetupPermissions() {
	const navigate = useNavigate();
	const { draft, change } = useSetup();
	const { commands, push } = draft.answers;
	const answered = commands !== undefined && push !== undefined;
	const agents = draft.members.filter((m) => m.on).map((m) => m.agent);
	const developers = agents.filter((a) => a.role === "software_developer");

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
			<div className={styles.card}>
				<Choice<Answer>
					name="commands"
					legend={t("mayCommands")}
					value={word(commands)}
					onChange={(v) => answer({ commands: v === "yes" })}
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
					onChange={(v) => answer({ push: v === "yes" })}
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
							<dd>{may(agent, draft)}</dd>
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
