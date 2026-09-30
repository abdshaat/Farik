import { AVATAR_URLS, type AvatarKey, Button, RoleTag } from "@farik/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { useQuery } from "../app/store.ts";
import { t } from "../strings/t.ts";
import styles from "./pages.module.css";
import type { Agent, Team as TeamFile } from "./setup/TeamSetup.tsx";

export type Model = { id: string; label: string };

/** The team file and the models the price table knows, as the Team pages read them. */
export function useTeam() {
	const { data } = useQuery<{ team: TeamFile }>("team.get", {});
	const { data: models } = useQuery<{ models: Model[] }>("models.list", {});
	return { team: data?.team, models: models?.models ?? [] };
}

/** The words for an agent's model, or its id when the price table has no newer label for it. */
export function modelLabel(agent: Agent, models: Model[]): string {
	const id = (agent.model as { id?: string } | undefined)?.id;
	if (!id) return t("agentModelRole");
	return models.find((m) => m.id === id)?.label ?? id;
}

/** Pause, resume or retire an agent through `agent_update`; answers the refusal, if any, in words. */
export function useStatus() {
	const { client } = useConnection();
	const [refusal, setRefusal] = useState<string>();
	const set = async (
		agentId: string,
		status: "active" | "paused" | "retired",
	): Promise<boolean> => {
		if (!client) return false;
		setRefusal(undefined);
		try {
			const reply = await client.command({
				command: "agent_update",
				body: { agentId, status },
			});
			if ("error" in reply) {
				setRefusal(reply.error.detail);
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
	const { team, models } = useTeam();
	const status = useStatus();
	if (!team) return null;
	const agents = team.agents.filter((a) => a.status !== "retired");
	return (
		<div className={styles.page}>
			<h1 className={styles.title}>{t("teamMembers")}</h1>
			<p>
				{t("teamPageLead")
					.replace("{count}", String(agents.length))
					.replace("{name}", team.name)}
			</p>
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
								{avatar && <img className={styles.face} src={avatar} alt="" />}
								<span>
									<strong>{name}</strong> <RoleTag role={agent.role} />
								</span>
							</div>
							<p>{agent.persona}</p>
							<p className={styles.muted}>
								{t(paused ? "agentPaused" : "agentActive")}
							</p>
							<dl className={styles.facts}>
								<dt>{t("agentModel")}</dt>
								<dd>{modelLabel(agent, models)}</dd>
							</dl>
							<div className={styles.actions}>
								<Link to={`/team/${agent.id}`}>
									{t("agentEditLink").replace("{name}", name)}
								</Link>
								<Button
									onClick={() =>
										status.set(agent.id, paused ? "active" : "paused")
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
			<section className={styles.section} aria-labelledby="changes-heading">
				<h2 id="changes-heading">{t("teamChangesTitle")}</h2>
				<p>{t("teamChangesBody")}</p>
			</section>
			<section className={styles.section} aria-labelledby="cost-heading">
				<h2 id="cost-heading">{t("teamCostTitle")}</h2>
				<p>
					{t("firstDay")} {t("teamCostNote")}
				</p>
			</section>
		</div>
	);
}
