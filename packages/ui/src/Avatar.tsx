import styles from "./Avatar.module.css";
import { AVATAR_URLS, type AvatarKey } from "./avatars.ts";

export function Avatar({
	avatarKey,
	name,
	size = 48,
}: {
	avatarKey: AvatarKey;
	name: string;
	size?: 32 | 48 | 64;
}) {
	return (
		<img
			className={styles.avatar}
			src={AVATAR_URLS[avatarKey]}
			alt={name}
			width={size}
			height={size}
		/>
	);
}
