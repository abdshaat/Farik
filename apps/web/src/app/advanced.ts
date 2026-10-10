import { useState } from "react";

const ADVANCED = "catervas.advanced";

// Storage can refuse (a private window): the switch then lives only in memory.
function readAdvanced(): boolean {
	try {
		return localStorage.getItem(ADVANCED) === "true";
	} catch {
		return false;
	}
}

/** The Advanced switch, shared by every page that shows detailed settings. */
export function useAdvanced(): [boolean, (on: boolean) => void] {
	const [advanced, setAdvanced] = useState(readAdvanced);
	const toggle = (on: boolean) => {
		try {
			localStorage.setItem(ADVANCED, String(on));
		} catch {}
		setAdvanced(on);
	};
	return [advanced, toggle];
}
