import styles from "../pages.module.css";
import { SavedTeams } from "../SavedTeams.tsx";

export function SavedTeamsPage() {
	return (
		<div className={styles.page}>
			<SavedTeams />
		</div>
	);
}
