import { Button, InfoTip } from "@catervas/ui";
import { useRef, useState } from "react";
import { useConnection } from "../app/connection.tsx";
import { Link } from "../app/links.tsx";
import { daemonSaid } from "../app/refusals.ts";
import { t } from "../strings/t.ts";
import { ServiceLogo } from "./ServiceLogo.tsx";
import type { Team } from "./setup/TeamSetup.tsx";
import type { KitService, RoleKit } from "./Team.tsx";
import styles from "./Today.module.css";

export type Suggestion = {
	agent: Team["agents"][number];
	service: KitService;
	/** The agent's own reason, when it asked for this service (ADR 0063). */
	why?: string;
	/** The service as the owner uses it with Claude, when finding the owner's projects found it. */
	found?: string;
};
export const SUGGESTIONS_SHOWN = 2;

type Pair = { agent: string; server: string };
type Requested = Pair & { why: string };
type Found = Pair & { name: string };

/**
 * The kit services an active agent lacks, in the team's order and each kit's order: not one the
 * agent holds under any source, not one signed in at launch only, not one the user said Not now to.
 * The ones an agent asked for come first, in the order asked, with its reason.
 */
export function suggestedConnections(
	team: Team,
	kits: RoleKit[],
	hidden: Pair[],
	requested: Requested[] = [],
	found: Found[] = [],
): Suggestion[] {
	const candidates = team.agents
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
	const asked = requested.flatMap((r) => {
		const found = candidates.find(
			(c) => c.agent.id === r.agent && c.service.name === r.server,
		);
		return found ? [{ ...found, why: r.why }] : [];
	});
	// A service the owner uses with Claude comes after the asked ones: it is a guess, theirs is a request.
	const used = found.flatMap((f) => {
		const match = candidates.find(
			(c) =>
				c.agent.id === f.agent &&
				c.service.name === f.server &&
				!asked.some((a) => a.agent === c.agent && a.service === c.service),
		);
		return match ? [{ ...match, found: f.name }] : [];
	});
	const listed = [...asked, ...used];
	return [
		...listed,
		...candidates.filter(
			(c) =>
				!listed.some((a) => a.agent === c.agent && a.service === c.service),
		),
	];
}

/** Today's "Suggested connections": nothing at all when there is nothing to suggest. */
export function SuggestedConnections(props: {
	team: Team;
	kits: RoleKit[];
	hidden: Pair[];
	requested?: Requested[];
	found?: Found[];
}) {
	const { client } = useConnection();
	const [gone, setGone] = useState<Pair[]>([]);
	const [open, setOpen] = useState(false);
	const [refusal, setRefusal] = useState<string>();
	const heading = useRef<HTMLHeadingElement>(null);
	const all = suggestedConnections(
		props.team,
		props.kits,
		[...props.hidden, ...gone],
		props.requested,
		props.found,
	);
	if (all.length === 0) return null;
	const shown = open ? all : all.slice(0, SUGGESTIONS_SHOWN);

	// The row goes at once; a refusal or failure puts it back.
	const notNow = async (pair: Pair) => {
		if (!client) return;
		setGone((g) => [...g, pair]);
		// The button that had focus goes with its row; the heading stays mounted.
		heading.current?.focus();
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
			<h2 id="suggestions-heading" ref={heading} tabIndex={-1}>
				{t("suggestionsTitle")}
			</h2>
			<ul
				id="suggestions-list"
				className={styles.rows}
				aria-label={t("suggestionsTitle")}
			>
				{shown.map(({ agent, service, why, found }) => {
					const id = `suggestion-${agent.id}-${service.name}`;
					return (
						<li key={id} className={styles.row}>
							<ServiceLogo name={service.name} />
							<div className={styles.rowText}>
								<strong id={id}>{service.title}</strong>
								<span>
									{why
										? t("suggestionsAsked", { agent: agent.displayName, why })
										: found
											? t("suggestionsFound", {
													service: found,
													agent: agent.displayName,
												})
											: t("suggestionsLine", {
													agent: agent.displayName,
													about: service.about,
												})}{" "}
									<InfoTip id={`${id}-info`}>{service.why}</InfoTip>
								</span>
							</div>
							<div className={styles.decide}>
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
							</div>
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
