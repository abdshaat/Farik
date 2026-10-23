import { Avatar, type AvatarKey } from "@catervas/ui";
import { Link } from "../app/links.tsx";
import type { Agent } from "../pages/setup/TeamSetup.tsx";
import styles from "./AgentPicture.module.css";

/** An agent's picture, which opens its page, `/team/<id>`; nothing when the agent has none. */
export function AgentPicture({
	agent,
	size = 32,
}: {
	agent: Agent | undefined;
	size?: 32 | 48;
}) {
	if (!agent?.avatar) return null;
	return (
		<Link to={`/team/${agent.id}`} className={styles.link}>
			<Avatar
				avatarKey={agent.avatar as AvatarKey}
				name={agent.displayName}
				size={size}
			/>
		</Link>
	);
}
