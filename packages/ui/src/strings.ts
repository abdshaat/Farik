import type { Role } from "./role.ts";

const roleShort: Record<Role, string> = {
	product_manager: "PM",
	scrum_master: "SM",
	architect: "ARCH",
	software_developer: "DEV",
	marketing_specialist: "MKT",
};

const roleName: Record<Role, string> = {
	product_manager: "Product Manager",
	scrum_master: "Scrum Master",
	architect: "Architect",
	software_developer: "Software Developer",
	marketing_specialist: "Marketing Specialist",
};

export const uiStrings = {
	close: "Close",
	noChanges: "No changes",
	added: "added",
	removed: "removed",
	stepOf: (n: number, m: number) => `Step ${n} of ${m}`,
	required: "required",
	busy: "Working…",
	roleShort,
	roleName,
};
