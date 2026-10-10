import { Avatar, type AvatarKey, RoleTag } from "@catervas/ui";
import { NavLink, useParams } from "react-router";
import { useQuery } from "../app/store.ts";
import { useWide } from "../shell/Shell.tsx";
import { t } from "../strings/t.ts";
import { Channel } from "./Channel.tsx";
import styles from "./Chats.module.css";
import { authorName, OneToOne, when } from "./OneToOne.tsx";
import { type Agent, roleName, type Team } from "./setup/TeamSetup.tsx";

/** A chat's newest line, as `chats.list` gives it. */
type Line = { seq: number; at: string; author: string; text: string };
type ChatsList = {
	teamLast: Line | null;
	chats: { agentId: string; retired: boolean; last: Line | null }[];
};

/** The chat list, Team first, beside (or, on a phone, above) the open chat: `/channel` or `/channel/<agent>`. */
export function Chats() {
	const { id } = useParams();
	const wide = useWide();
	const { data: team } = useQuery<{ team: Team }>("team.get", {});
	const { data: list } = useQuery<ChatsList>("chats.list", {});
	// Team's chat does not wait for the team: it asks for it itself, at once.
	const agents = team?.team.agents;
	const agent = agents?.find((a) => a.id === id);
	return (
		<div className={wide ? styles.wide : styles.narrow}>
			{agents && <ChatPicker list={list} agents={agents} wide={wide} />}
			<div className={styles.open}>
				{id === undefined ? (
					<Channel />
				) : agent && agents ? (
					<OneToOne key={agent.id} agent={agent} agents={agents} />
				) : (
					agents && <h1 className={styles.none}>{t("chatsNoSuch")}</h1>
				)}
			</div>
		</div>
	);
}

function ChatPicker({
	list,
	agents,
	wide,
}: {
	list: ChatsList | undefined;
	agents: Agent[];
	wide: boolean;
}) {
	// The team's order; an agent the list does not hold yet (it is still loading) shows with no line.
	const rows = agents.map((agent) => ({
		agent,
		last: list?.chats.find((c) => c.agentId === agent.id)?.last ?? null,
	}));
	const current = rows.filter((r) => r.agent.status !== "retired");
	const past = rows.filter((r) => r.agent.status === "retired");
	const team = list?.teamLast;

	if (!wide)
		return (
			<nav aria-label={t("chats")} className={styles.faces}>
				<ul>
					<li>
						<NavLink to="/channel" end>
							<TeamFace />
							<span>{t("channel")}</span>
						</NavLink>
					</li>
					{[...current, ...past].map(({ agent }) => (
						<li key={agent.id}>
							<NavLink to={`/channel/${agent.id}`}>
								{/* The name below says who it is. */}
								<span aria-hidden="true">
									<Face agent={agent} />
								</span>
								<span>{agent.displayName}</span>
							</NavLink>
						</li>
					))}
				</ul>
			</nav>
		);

	const row = (agent: Agent, last: Line | null) => {
		const name = agent.displayName;
		const said =
			agent.status === "retired"
				? t("chatsGone", { name })
				: last
					? t("chatsSaid", {
							name: authorName(last.author, agents),
							text: last.text,
						})
					: t("chatsNone", { name });
		return (
			<li key={agent.id}>
				<NavLink to={`/channel/${agent.id}`} className={styles.row as string}>
					<Face agent={agent} />
					<span className={styles.who}>
						<span className={styles.top}>
							<strong>{name}</strong>
							<RoleTag role={agent.role} />
							<span className={styles.role}>{roleName(agent.role)}</span>
							{last && (
								<time className={styles.time} dateTime={last.at}>
									{when(last.at, true)}
								</time>
							)}
						</span>
						<span className={styles.last}>{said}</span>
					</span>
				</NavLink>
			</li>
		);
	};
	return (
		<nav aria-labelledby="chats-title" className={styles.list}>
			<h2 id="chats-title" className={styles.title}>
				{t("chats")}
			</h2>
			<ul>
				<li>
					<NavLink to="/channel" end className={styles.row as string}>
						<TeamFace />
						<span className={styles.who}>
							<span className={styles.top}>
								<strong>{t("channel")}</strong>
								<span className={styles.role}>{t("chatsEveryone")}</span>
								{team && (
									<time className={styles.time} dateTime={team.at}>
										{when(team.at, true)}
									</time>
								)}
							</span>
							{team && (
								<span className={styles.last}>
									{t("chatsSaid", {
										name: authorName(team.author, agents),
										text: team.text,
									})}
								</span>
							)}
						</span>
					</NavLink>
				</li>
				{current.map((r) => row(r.agent, r.last))}
			</ul>
			{past.length > 0 && (
				<>
					<h3 className={styles.past}>{t("chatsPast")}</h3>
					<ul>{past.map((r) => row(r.agent, r.last))}</ul>
				</>
			)}
		</nav>
	);
}

function Face({ agent }: { agent: Agent }) {
	return agent.avatar ? (
		<Avatar
			avatarKey={agent.avatar as AvatarKey}
			name={agent.displayName}
			size={48}
		/>
	) : (
		<span className={styles.blank} />
	);
}

/** Team's face: two people, on the rail's dark band. */
function TeamFace() {
	return (
		<span className={styles.face}>
			<svg
				viewBox="0 0 24 24"
				width="24"
				height="24"
				fill="none"
				aria-hidden="true"
			>
				<circle cx="9" cy="9" r="3" stroke="currentColor" strokeWidth="2" />
				<circle
					cx="16.5"
					cy="10"
					r="2.5"
					stroke="currentColor"
					strokeWidth="2"
				/>
				<path
					d="M3.5 19c.8-3 3-4.5 5.5-4.5s4.7 1.5 5.5 4.5M15 15c2.6-.4 4.6.8 5.5 3.5"
					stroke="currentColor"
					strokeWidth="2"
					strokeLinecap="round"
				/>
			</svg>
		</span>
	);
}
