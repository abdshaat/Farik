import { Button, InfoTip } from "@catervas/ui";
import { useState } from "react";
import { Link } from "react-router";
import { useConnection } from "../app/connection.tsx";
import { daemonSaid } from "../app/refusals.ts";
import { t } from "../strings/t.ts";
import { ServiceLogo } from "./ServiceLogo.tsx";
import type { Team } from "./setup/TeamSetup.tsx";
import type { KitService, RoleKit } from "./Team.tsx";
import styles from "./Today.module.css";

export type Suggestion = { agent: Team["agents"][number]; service: KitService };
export const SUGGESTIONS_SHOWN = 4;

type Pair = { agent: string; server: string };

/**
 * The kit services an active agent lacks, in the team's order and each kit's order: not one the
 * agent holds under any source, not one signed in at launch only, not one the user said Not now to.
 */
export function suggestedConnections(
	team: Team,
	kits: RoleKit[],
	hidden: Pair[],
): Suggestion[] {
	return team.agents
		.filter((agent) => agent.status === "active")
		.flatMap((agent) =>
			(kits.find((k) => k.role === agent.role)?.connectors ?? [])
				.filter(
					(service) =>
						!service.atLaunch &&
						!agent.mcpServers?.some((m) => m.name === service.name) &&
						!hidden.some(
							(h) => h.agent === agent.id && h.server === service.name,
						),
				)
				.map((service) => ({ agent, service })),
		);
}

/** Today's "Suggested connections": nothing at all when there is nothing to suggest. */
export function SuggestedConnections(props: {
	team: Team;
	kits: RoleKit[];
	hidden: Pair[];
}) {
	const { client } = useConnection();
	const [gone, setGone] = useState<Pair[]>([]);
	const [open, setOpen] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const all = suggestedConnections(props.team, props.kits, [
		...props.hidden,
		...gone,
	]);
	if (all.length === 0) return null;
	const shown = open ? all : all.slice(0, SUGGESTIONS_SHOWN);

	// The row goes at once; a refusal or failure puts it back.
	const notNow = async (pair: Pair) => {
		if (!client) return;
		setGone((g) => [...g, pair]);
		try {
			await client.call("connector_suggestion.hide", pair);
			setRefusal(undefined);
		} catch (error) {
			setGone((g) => g.filter((p) => p !== pair));
			setRefusal(daemonSaid(error, "refuseCommand"));
		}
	};

	return (
		<section className={styles.section} aria-labelledby="suggestions-heading">
			<h2 id="suggestions-heading">{t("suggestionsTitle")}</h2>
			<ul
				id="suggestions-list"
				className={styles.rows}
				aria-label={t("suggestionsTitle")}
			>
				{shown.map(({ agent, service }) => {
					const id = `suggestion-${agent.id}-${service.name}`;
					return (
						<li key={id} className={styles.row}>
							<ServiceLogo name={service.name} />
							<div className={styles.rowText}>
								<strong id={id}>{service.title}</strong>
								<span>
									{t("suggestionsLine", {
										agent: agent.displayName,
										about: service.about,
									})}{" "}
									<InfoTip id={`${id}-info`}>{service.why}</InfoTip>
								</span>
							</div>
							<Link
								className={styles.action}
								to={`/team/${agent.id}?connect=${service.name}`}
								aria-describedby={id}
							>
								{t("suggestionsConnect")}
							</Link>
							<Button
								kind="quiet"
								disabled={!client}
								onClick={() =>
									notNow({ agent: agent.id, server: service.name })
								}
							>
								{t("suggestionsNotNow")}
							</Button>
						</li>
					);
				})}
			</ul>
			{all.length > SUGGESTIONS_SHOWN && (
				<Button
					kind="quiet"
					expanded={open}
					controls="suggestions-list"
					onClick={() => setOpen(!open)}
				>
					{open
						? t("suggestionsShowFewer")
						: t("suggestionsShowAll", { count: all.length })}
				</Button>
			)}
			{refusal && (
				<p role="alert" className={styles.muted}>
					{refusal}
				</p>
			)}
		</section>
	);
}
