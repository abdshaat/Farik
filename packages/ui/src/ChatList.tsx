import { Avatar } from "./Avatar.tsx";
import type { AvatarKey } from "./avatars.ts";
import styles from "./ChatList.module.css";
import { RoleTag } from "./RoleTag.tsx";
import type { Role } from "./role.ts";

export function ChatList({
	label,
	messages,
}: {
	label: string;
	messages: {
		id: string;
		author: { name: string; role?: Role; avatarKey?: AvatarKey };
		time: string;
		text: string;
		thread?: string;
	}[];
}) {
	return (
		<ol className={styles.list} aria-label={label}>
			{messages.map((m) => (
				<li key={m.id} className={styles.message}>
					{m.author.avatarKey ? (
						<Avatar
							avatarKey={m.author.avatarKey}
							name={m.author.name}
							size={32}
						/>
					) : null}
					<div className={styles.body}>
						<p className={styles.meta}>
							<strong>{m.author.name}</strong>
							{m.author.role ? <RoleTag role={m.author.role} /> : null}
							<span className={styles.time}>{m.time}</span>
							{m.thread ? (
								<span className={styles.thread}>{m.thread}</span>
							) : null}
						</p>
						<p className={styles.text}>{m.text}</p>
					</div>
				</li>
			))}
		</ol>
	);
}
