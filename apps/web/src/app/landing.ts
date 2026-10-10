import type { ServeStatus } from "./store.ts";

/** Where "/" goes: setup until Farik has a project and its team, Today after. */
export function landing(status: ServeStatus): string {
	// A project was chosen but could not be taken on (its screen says why), or is being left.
	if (status.projectRoot === null)
		return status.takeOnError || status.leaving
			? "/setup/project"
			: "/setup/computer";
	return status.setupPending ? "/setup/scan" : "/";
}
