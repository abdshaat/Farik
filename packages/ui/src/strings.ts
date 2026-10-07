import type { Role } from "./role.ts";

const roleShort: Record<Role, string> = {
	product_manager: "PM",
	scrum_master: "SM",
	architect: "ARCH",
	software_developer: "DEV",
	marketing_specialist: "MKT",
	ui_ux_designer: "UX",
	finance_specialist: "FIN",
	procurement_specialist: "PROC",
};

const roleName: Record<Role, string> = {
	product_manager: "Product Manager",
	scrum_master: "Scrum Master",
	architect: "Architect",
	software_developer: "Developer",
	marketing_specialist: "Marketing Specialist",
	ui_ux_designer: "UI/UX Designer",
	finance_specialist: "Finance Specialist",
	procurement_specialist: "Procurement Specialist",
};

export const uiStrings = {
	close: "Close",
	noChanges: "No changes",
	renamed: "Renamed, no other change",
	binary: "A binary file changed; it cannot be shown",
	added: "added",
	removed: "removed",
	stepOf: (n: number, m: number) => `Step ${n} of ${m}`,
	required: "required",
	busy: "Working…",
	roleShort,
	roleName,
};
