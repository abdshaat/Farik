import styles from "./RoleTag.module.css";
import type { Role } from "./role.ts";
import { uiStrings } from "./strings.ts";

const tone: Record<Role, string> = {
	product_manager: styles.productManager as string,
	scrum_master: styles.scrumMaster as string,
	architect: styles.architect as string,
	software_developer: styles.developer as string,
	marketing_specialist: styles.marketingSpecialist as string,
	ui_ux_designer: styles.uiUxDesigner as string,
	finance_specialist: styles.financeSpecialist as string,
};

export function RoleTag({ role }: { role: Role }) {
	return (
		<abbr
			className={`${styles.tag} ${tone[role]}`}
			title={uiStrings.roleName[role]}
		>
			{uiStrings.roleShort[role]}
		</abbr>
	);
}
