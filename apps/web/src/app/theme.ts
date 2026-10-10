import { useEffect, useState } from "react";

export type ThemeChoice = "light" | "dark" | "system";

const KEY = "catervas.theme";
const DARK = "(prefers-color-scheme: dark)";

// Storage can refuse (a private window, blocked site data): the theme then lives only in memory.
function stored(): ThemeChoice {
	try {
		const v = localStorage.getItem(KEY);
		return v === "dark" || v === "system" ? v : "light";
	} catch {
		return "light";
	}
}

function store(c: ThemeChoice): void {
	try {
		localStorage.setItem(KEY, c);
	} catch {}
}

export function useTheme(): [ThemeChoice, (c: ThemeChoice) => void] {
	const [choice, setChoice] = useState(stored);
	useEffect(() => {
		const root = document.documentElement;
		if (choice !== "system") {
			root.dataset.theme = choice;
			return;
		}
		const query = matchMedia(DARK);
		const follow = () => {
			root.dataset.theme = query.matches ? "dark" : "light";
		};
		follow();
		query.addEventListener("change", follow);
		return () => query.removeEventListener("change", follow);
	}, [choice]);
	const choose = (c: ThemeChoice) => {
		store(c);
		setChoice(c);
	};
	return [choice, choose];
}
